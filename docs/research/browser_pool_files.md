# Chromium/WebView2 User-Data-Profil-Dateien – Klassifikation für Linked-Clone/Delta-Modell

## (A) READ-ONLY – Kann verlinkt werden (statisch / shared)
- `Extensions/` – Installierte Erweiterungen (unveränderlich nach Installation)
- `pnacl/` – Portable Native Client Komponenten
- `Subresource Filter/` – Filterregeln für Ressourcen
- `WidevineCdm/` – DRM-Modul
- `MEIPreload/` – Media Engagement Index (vorausgefüllt)
- `resources.pak`, `*.pak` – Lokalisierungs- und UI-Ressourcen
- `chrome_100_percent.pak`, `chrome_200_percent.pak` – Bildschirmskalierungs-Assets
- `icudtl.dat` – Internationalisierungskomponenten
- `snapshot_blob.bin` / `v8_context_snapshot.bin` – V8-Startup-Snapshots

**Begründung:** Diese Dateien werden nur gelesen, nie von der Chromium-Instanz beschrieben, und sind versionsstabil. Sie können über Hardlinks oder symlinks geteilt werden, ohne dass Schreibkonflikte oder Seiteneffekte entstehen.

## (B) MUTABLE – Muss pro Instanz delta-kopiert werden (instanzspezifisch)
- `Cookies` / `Cookies-journal` – Sitzungs- und persistente Cookies
- `Login Data` / `Login Data-journal` – Gespeicherte Anmeldeinformationen
- `Web Data` / `Web Data-journal` – Autofill-Daten, Kreditkarten, Adressen
- `Local State` – Instanzkonfiguration (u.a. Geräte-ID, statistische IDs)
- `IndexedDB/` – Lokale NoSQL-Datenbanken
- `Service Worker/` – Service Worker Skripte und Caches
- `Cache/` / `Code Cache/` – HTTP-, Script- und Bild-Caches
- `Session Storage/` – Session- (flüchtig) und `Local Storage/` – persistente DOM-Storage
- `Network/` – Netzwerkzustand, HSTS, TLS-Sitzungen, Quic
- `Preferences` – Benutzereinstellungen (nicht synchronisiert)
- `Secure Preferences` – Sicherheitsrelevante Einstellungen
- `DataStore/` – Verschiedene instanzspezifische Daten

**Begründung:** Diese Dateien werden regelmäßig beschrieben, enthalten Benutzerzustände oder instanzspezifische IDs. Ein Teilen würde zu Korruption, Singleton-Lock-Konflikten und Datenvermischung führen.

## Minimale Delta-Kopie für geteilten Login ohne SingletonLock-Konflikt
**Kopiere pro Instanz:**
- `Cookies`
- `Login Data`
- `Web Data`
- `Local State`
- `Preferences`
- `IndexedDB/` (nur wenn Login-spezifische Daten genutzt werden)
- `Local Storage/` (nur wenn App-Zustand benötigt wird)

**Warum diese minimale Liste:**
- `Cookies` + `Login Data` sind die beiden essenziellen Dateien für persistente Logins. Sie enthalten die notwendigen HTTP-Cookies und verschlüsselten Anmeldeinformationen.
- `Local State` liefert die Geräte-ID – für viele Dienste (z.B. Google) notwendig, um den Login als „gleiches Gerät“ zu erkennen und so die 2FA-Abfragen zu reduzieren.
- `Web Data` sorgt dafür, dass Autofill-Daten (Adressen, Kreditkarten) für den Nutzer verfügbar bleiben.
- `Preferences` (falls vorhanden) – sonst fällt die Instanz auf Standardwerte zurück, was meist akzeptabel ist, aber bei manchen Diensten die Login-Erkennung beeinflussen kann.
- `IndexedDB` und `Local Storage` sind optional, je nachdem ob die WebApp zustandsbehaftete Daten speichert.

**Wichtig:** Alle anderen Mutable-Dateien (`Cache/`, `Network/`, `Service Worker/`, `Session Storage/`, `Code Cache/`) müssen **nicht** kopiert werden – sie werden bei Bedarf neu erzeugt und sind für den Login-Status nicht relevant. Durch Weglassen dieser großen Cache- und Netzwerkzustände vermeidest du SingletonLock-Konflikte und reduzierst den Speicherbedarf erheblich.
