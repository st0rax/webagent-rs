## Auftrag

Closes #

## Ergebnis

<!-- Beobachtbares Ergebnis, nicht nur ausgeführte Tätigkeit. -->

## Scope und bewusste Nicht-Ziele

<!-- Geänderte Komponenten sowie ausdrücklich ausgelassene Bereiche. -->

## Evidenz

<!-- Tatsächlich ausgeführte Befehle mit Ergebnis; keine Secrets oder privaten Laufzeitdaten. -->

```text
cargo fmt --all -- --check
cargo clippy --no-default-features --all-targets -- -D warnings
cargo test --no-default-features
git diff --check
```

## Risiken und Wiederherstellung

<!-- Plattform-, Persistenz-, Sicherheits- und Featuregrenzen; Rücknahmeweg. -->

## Übergabe

<!-- Offene Kriterien, Blocker und genau eine sicherste nächste Aktion. -->

- [ ] Das verknüpfte Issue enthält Auftrag und Abnahmekriterien.
- [ ] Der Diff enthält keine fremden oder generierten Dateien.
- [ ] Relevante Tests und Gates sind oben mit Ergebnis dokumentiert.
- [ ] `docs/CURRENT_WORK.md` ist auf diesen Stand aktualisiert.
- [ ] Keine Credentials, Cookies, Profile oder privaten Transkripte sind enthalten.
- [ ] Externe Aktionen wurden nur im ausdrücklich freigegebenen Umfang ausgeführt.
- [ ] Push/Release/Deployment werden nicht aus lokaler Fertigstellung abgeleitet.
