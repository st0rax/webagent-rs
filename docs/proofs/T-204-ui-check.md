# T-204 UI-Prüfung

Datum: 2026-09-06

Die lokale `web/index.html` wurde über einen statischen Server geladen. Sichtbar und erreichbar waren der dunkle Workspace-Header mit Health-Balken, die linke Workspace-/Sitzungsnavigation, der zentrale Chat-Header mit Brain-Status, der Chatbereich, die Aktivitätsleiste und der Composer.

Geprüfte Interaktion: Eine Beispielnachricht (`Wie ist der aktuelle Status?`) wurde über den Composer abgeschickt. Der Fake-Stream erzeugte die Benutzerkarte, die Claude-Antwort und die Aktivitätsereignisse `Fake-Textdelta` sowie `Stop: Fake-Stream beendet`. Die Antwort endete mit `Antwort fertig`.

Die bestehenden A11y-Strukturen blieben erhalten: Skip-Link, semantische Buttons, `:focus-visible`, `aria-live`, reduzierte Bewegung und beschriftete Eingabe. Es wurde kein Backend-Aufruf ausgelöst.

Verfügbare Prüfungen: `git diff --check`, JSON-Syntax, Web-Asset-Präsenz und Handover-Verweise bestanden. Der Rust-Test-Gate konnte in der Sandbox nicht ausgeführt werden, weil `cargo` nicht installiert ist. Ein separater schmaler Viewport-Durchlauf und der Rust-Test-Gate bleiben vor Abschluss des Tasks offen.
