# **Referenz.** WebAgent Play-Mode v0.3 — Protocol Detective

Status: Umsetzungsgrundlage, **kein** Abnahmebeleg. Gehört zu
`docs/TASKBOARD.json` (T-978..T-996, Schema v2). Basis: Konzept v0.2
(2026-11-26) plus fachliche Korrekturen und Typdefinitionen. Geprüfte
Versionszeile im Repo: `Cargo.toml:3` = `0.11.7`. Bestand:
[`docs/play/RECON.md`](play/RECON.md).

## 0. Verbindliche Grenzen

- Dieses Dokument beschreibt den **Zielzustand**. Der echte Code liegt Stand
  2026-11-26 in `src/repl/play.rs`; `src/play/` gibt es noch nicht. Die
  Task-Scopes sind in der Tafel auf die echten Pfade korrigiert.
- Ein Task, eine Änderung, nur die Dateien aus `scope`. Ein Claim ist nur
  zulässig, wenn alle `depends_on` den Status `done` haben; das wird von
  `acquire_claim` erzwungen.
- Kein Task verändert den Core (`controller`, `brain`, `executor`). Die
  wenigen Tasks, die `src/repl/play.rs` berühren, nennen das ausdrücklich in
  ihrem `scope`.
- Tests laufen ohne Netzwerk. Live-Zugriffe nur hinter `play-live` (T-994).
- Spieldaten sind synthetisch (Abschnitt 7). Keine echten Accounts, DIDs oder
  Tokens.
- XP gibt es ausschließlich für echte Observations aus dem vorhandenen
  `ObservationHook`. Es gibt genau einen Beleg-Gate.

## 1. Korrekturen gegenüber v0.2

| Thema | v0.2 | v0.3 |
|---|---|---|
| Handle-Beweis am PDS | `app.bsky.actor.profile` enthalte den Handle | Das Profil-Record enthält **keinen** Handle. Beweis über `com.atproto.repo.describeRepo` (`handle`, `didDoc`, `handleIsCorrect`), Handle-Resolution (DNS-TXT `_atproto.<handle>` bzw. `https://<handle>/.well-known/atproto-did`) und PLC-Audit-Log. |
| Identity-Event | „nicht im PLC-Audit-Log signiert" | Ein Identity-Event ist ein Hinweis, neu aufzulösen; es trägt keine eigene Signatur. Der Geist entsteht, wenn die Neuauflösung nicht zum Event passt. |
| Jetstream-Endpunkt | `subscribeEvents` | `wss://<host>/subscribe` mit `wantedCollections`, `wantedDids`, `cursor`. Jetstream ist ein eigener Dienst, nicht der Firehose-XRPC-Endpunkt. |
| Account-Status | fehlt | Neuer Geist `Status`: AppView sieht eine DID nicht, weil das Konto deaktiviert, gesperrt oder takendown ist. Beweis über `com.atproto.sync.getRepoStatus` am PDS oder Jetstream-`account`-Event (nur `Unverified`). |
| Brain-Wechsel | `ghosts += 1` ohne Bindungsweg | Eigener Geist `BrainDrift`, Bindung durch Re-Verify (Abschnitt 5). |
| Ausfall 16.08.2026 | als Fakt in der Story | Nur Story-Hintergrund ohne Anspruch auf Realität, nicht als verifizierte Aussage. |
| Session-Zusammenfassung | „2 Einträge" bei 1 Verified + 1 Verworfen | Konsistent: 2 Einträge (1 `Verified`, 1 `Refuted`). |
| Versionszeile | `v0.11.6` mit Tag `v0.11.5` | Im Repo geprüft: `0.11.7`. |

## 2. Kernidee

Verifizierbarkeit ist die Ressource. Billige Quellen (Jetstream) sind schnell,
aber nie beweiskräftig. Teure Quellen (PDS, PLC, Handle-Resolution) sind
autoritativ. Ein Tagebucheintrag zählt nur als „verifiziert", wenn er auf
einen autoritativen, erfolgreichen und passenden Proof verweist. Das ist die
Anti-Halluzinations-Regel und wird vom Evaluate-Schritt maschinell geprüft.

## 3. Proof-Modul (`src/play/proof.rs`, T-979)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofSource {
    Pds,
    Plc,
    HandleResolution,
    Firehose,       // kryptografisch prüfbar (CAR/MST), später
    Constellation,  // indexiert
    AppView,        // indexiert
    Jetstream,      // unverifiziert
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    Authoritative,
    Indexed,
    Unverified,
}

pub const fn authority(s: ProofSource) -> Authority {
    match s {
        ProofSource::Pds
        | ProofSource::Plc
        | ProofSource::HandleResolution
        | ProofSource::Firehose => Authority::Authoritative,
        ProofSource::Constellation | ProofSource::AppView => Authority::Indexed,
        ProofSource::Jetstream => Authority::Unverified,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProofId(pub String); // Format "P-0001"

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proof {
    pub id: ProofId,
    pub source: ProofSource,
    pub subject: String,          // did, handle oder at-uri
    pub kind: String,             // z. B. "plc_audit", "describe_repo", "jetstream_identity"
    pub ok: bool,                 // Abruf erfolgreich
    pub observed_at: String,      // RFC 3339; in Fixtures fest
    pub fixture: bool,
    pub summary: String,          // knappe Beobachtung für die Anzeige
    pub raw_ref: Option<String>,  // Pfad/Schlüssel der Rohdaten
}
```

`Firehose` bleibt im Enum für spätere Ausbaustufen; kein Task in diesem Plan
erzeugt Firehose-Proofs. `src/lib.rs` erhält in T-979 genau einmal
`pub mod play;`.

## 4. Claims (`src/play/claim.rs`, T-980)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Claim {
    HandleBindsDid { handle: String, did: String },
    HandleUnbound { did: String },
    PdsIs { did: String, pds: String },
    RecordExists { uri: String },
    RecordAbsent { uri: String },
    RecordCid { uri: String, cid: String },
    AccountStatus { did: String, active: bool, status: Option<String> },
    AppviewMatchesPds { uri: String, matches: bool },
}

pub enum Support { Confirms, Refutes, Unrelated }

pub fn supports(claim: &Claim, proof: &Proof) -> Support;
```

`proof.kind` bestimmt die Auswertung:

| Claim | Bestätigt durch | Widerlegt durch |
|---|---|---|
| `HandleBindsDid` | `describe_repo` mit `handleIsCorrect=true` und gleichem Handle, oder `handle_resolution` mit gleicher DID | `describe_repo` mit `handleIsCorrect=false`, `handle_resolution` auf andere DID oder 404, `plc_audit` ohne Handle in `alsoKnownAs` |
| `HandleUnbound` | `plc_audit` letzter Stand ohne Handle | `plc_audit` letzter Stand mit Handle und passender Resolution |
| `PdsIs` | `plc_audit` letzter Stand mit passendem `services.atproto_pds` | anderer PDS im letzten Stand |
| `RecordExists` / `RecordAbsent` | `get_record` ok / 404 `RecordNotFound` | jeweils umgekehrt |
| `AccountStatus` | `repo_status` mit gleichen Werten | abweichende Werte |
| `AppviewMatchesPds` | zwei Proofs (AppView + PDS) mit gleichem Inhalt | zwei Proofs mit Abweichung |

Ein Proof mit `ok == false` ergibt **nie** `Confirms`.

## 5. Tagebuch und Geister

### 5.1 `DiaryEntry` (`src/play/diary.rs`, T-982)

```rust
pub struct DiaryEntry {
    pub id: String,                    // "D-0001"
    pub aussage: String,
    pub claim: Claim,
    pub quelle: ProofSource,
    pub verifiziert: bool,
    pub gegenbeweis: Option<ProofId>,  // Referenz, kein Freitext
    pub spekulativ: bool,
    pub kontaminiert: bool,
    pub naechster_schritt: Option<String>,
    pub tick: u64,
}

pub enum Verdict { Verified, Speculative, Refuted }

pub fn validate_entry(ledger: &Ledger, e: &mut DiaryEntry) -> Verdict;
```

Regeln:

1. `gegenbeweis` zeigt auf einen existierenden Proof mit
   `authority == Authoritative`, `ok == true` und
   `supports(claim, proof) == Confirms` → `Verified`, `verifiziert = true`,
   `spekulativ = false`.
2. Existiert irgendein autoritativer Proof mit `Refutes` → `Refuted`,
   `verifiziert = false`.
3. Sonst → `Speculative`, `verifiziert = false`, `spekulativ = true`. Zählt
   nicht für den Fallabschluss.

### 5.2 Geister (`src/play/ghost.rs`, T-984)

Binden heißt: Ursache diagnostiziert — auch wenn sich die Behauptung als wahr
herausstellt (`Confirmed`).

| `GhostKind` | Entsteht durch | Bindung | Outcome |
|---|---|---|---|
| `Identity` | Jetstream-`identity`-Event, das einer Neuauflösung nicht entspricht oder noch nicht geprüft ist | autoritativer PLC-Proof **und** (`describe_repo`- oder `handle_resolution`-Proof) | `Confirmed`, wenn beide zum Event passen, sonst `Refuted` |
| `UnsignedCommit` | Jetstream-`commit` für einen ungeprüften Record | `get_record`-Proof am PDS (ok, auch bei 404) | `Confirmed`, wenn Record existiert und CID passt, sonst `Refuted` |
| `AppviewSplit` | AppView zeigt Record/Profil, das PDS anders oder gar nicht (oder umgekehrt) | AppView-Proof und PDS-Proof zum selben Subject | `Confirmed`, wenn die Abweichung belegt ist, sonst `Refuted` |
| `Status` | AppView sieht eine DID nicht, PDS läuft | `repo_status`-Proof am PDS | `Confirmed` bei inaktivem Konto, sonst `Refuted` |
| `BrainDrift` | Brain-Wechsel | neuer autoritativer Proof, der den zuletzt verifizierten Diary-Eintrag erneut bestätigt (Re-Verify) | immer `Confirmed` bei Erfolg |

Jetstream-Proofs binden **nie**. Rauschen und Missionsdruck aus v0.2 bleiben
bestehen, solange ein Geist unbound ist. Ein Eintrag, der unter einem
ungebundenen Geist ohne autoritativen Proof geschrieben wird, bekommt
`kontaminiert = true`.

`Save` bekommt `diary: Vec<DiaryEntry>` und `ghost_log: Vec<Ghost>`, beide
`#[serde(default)]`. Der bisherige `ghosts`-Wert wird zur abgeleiteten Zahl
der ungebundenen Geister und nicht mehr separat hochgezählt. Save-Version
1 → 2 mit Migration beim Laden (T-983); `Save::write` wird dabei atomar
(tmp + rename), weil es das heute nicht ist.

## 6. Missionen und Räume

| Mission | Raum | Abschlusskriterium | Badge-Idee |
|---|---|---|---|
| `Tutorial` | `prompt#` | mindestens ein autoritativer PLC-Proof mit `ok` | `startschein` |
| `Scan` | `breaker/` (PLC) | ein Identity-Geist wurde erzeugt und gebunden, Diary-Eintrag `Verified` oder `Refuted` | `audit-leser` |
| `Repair` | `bridge>` (AppView ↔ PDS) | Status- oder AppviewSplit-Geist gebunden | `naht-pruefer` |
| `Work` | `werkbank/` | vorerst unverändert (T-995 liefert das Design) | – |
| `Champion` | `arena/` (Jetstream) | alle Geister gebunden, mindestens 5 `Verified`-Einträge, `final_text` vorhanden | `beweiskette` |

Räume bleiben über `exits()` verbunden. Rauschsätze (T-991) sind deutsch,
deterministisch und je Geisterart getrennt.

## 7. Fall `case001` (synthetisch)

Keine echten Accounts. DID `did:plc:ksn4wq3zbxe7rtgp2vhf5ydj` (24 Zeichen,
Alphabet `a-z2-7`), Handle (alt) `annie.example.test`, PDS (neu)
`https://pds.example.test`.

- PLC-Audit-Log: 2026-10-01 Handle gesetzt, 2026-10-12 Handle entfernt,
  2026-10-14 PDS gewechselt.
- `describeRepo`: `handle: "handle.invalid"`, `handleIsCorrect: false`.
- Handle-Resolution `annie.example.test`: 404.
- Jetstream: ein `identity`-Event mit `handle: "annie.example.test"` für
  dieselbe DID.

Fixture-Dateien: `play/fixtures/case001/{plc_did.json,plc_audit.json,
describe_repo.json,handle_resolution_annie.json,jetstream.jsonl,failsafe.json}`.

Referenzsequenz (Golden-Test T-993):

1. Start: kein Save, Kern `big-pickle`, Raum `prompt#`. Failsafe 001 wird
   fällig, nennt als Handle noch `annie.example.test` (`Reliability::Wrong`,
   weil überholt) und fordert, die DID aufzulösen.
2. `resolve did:plc:ksn4…` → Proof `P-0001` (`Plc`, `ok`). Diary `D-0001`:
   Claim `HandleUnbound`, `gegenbeweis = P-0001` → `Verified`.
3. Jetstream-`identity` → Proof `P-0002` (`Jetstream`, `Unverified`). Geist
   `G-0001` (`Identity`) entsteht. Diary `D-0002`: Claim
   `HandleBindsDid`, nur `P-0002` → `Speculative`; wird in Schritt 4 korrigiert.
4. `describe_repo` am PDS → `P-0003` (`Pds`, `ok`,
   `handleIsCorrect=false`). `resolve_handle annie.example.test` →
   `P-0004` (`HandleResolution`, `ok`, 404 = unbound). `G-0001` wird mit
   `Refuted` gebunden; `D-0002` → `Refuted`.
5. Endzustand: 2 Einträge (1 `Verified`, 1 `Refuted`), 0 ungebundene
   Geister, Raum `bridge>` frei, Missionen `Tutorial` und `Scan` erfüllt.

## 8. Live-Endpunkte (nur Feature `play-live`, T-994)

| Zweck | Endpunkt |
|---|---|
| DID-Dokument | `GET https://plc.directory/{did}` |
| Audit-Log | `GET https://plc.directory/{did}/log/audit` |
| `describeRepo` | `GET {pds}/xrpc/com.atproto.repo.describeRepo?repo={did}` |
| `getRecord` | `GET {pds}/xrpc/com.atproto.repo.getRecord?repo={did}&collection=…&rkey=…` |
| Repo-Status | `GET {pds}/xrpc/com.atproto.sync.getRepoStatus?did={did}` |
| Handle-Resolution | `GET https://{handle}/.well-known/atproto-did` (DNS-TXT optional) |
| AppView-Profil | `GET https://public.api.bsky.app/xrpc/app.bsky.actor.getProfile?actor={did}` |
| Jetstream | `wss://jetstream1.us-east.bsky.network/subscribe?wantedCollections=…&cursor=…` |

`LiveSource` muss Timeout 10 s, User-Agent `webagent-play`, Backoff bei 429,
Antwortgröße maximal 1 MiB und ausschließlich `https`/`wss` verwenden. Die
Endpunkte sind vor der Umsetzung gegen die aktuelle Dokumentation zu prüfen;
Parameter können sich ändern.

## 9. Offene Punkte nach diesem Plan

- Werkbank/Constellation: T-995 liefert nur das Design.
- Firehose-Proofs mit CAR/MST-Prüfung als eigene Ausbaustufe.
- Weitere Fälle (`case002` …) und Fallauswahl im Menü.
- Die engen Testfilter `play::proof` … `play::live` finden Stand 2026-11-26
  **0 Tests**; sie sind Ziele der Tasks, keine Belege (Nachweis in
  `docs/play/RECON.md`).
