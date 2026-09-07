# DeepSeek: Segmentleiste als modellaequivalente Auswahl

> **Abnahmeentscheidung.** Der Nutzer hat am 2026-09-07 festgelegt, dass
> DeepSeeks `Instant | Expert | Vision` im Zweifelsfall als Modellwechsel
> behandelt wird. Das ändert die Semantik der Matrixzelle `model/deepseek`,
> nicht den Belegstandard.

## Ausgangslage

Der aktuelle DeepSeek-Chat zeigt drei dauerhaft sichtbare Inferenzvarianten
statt eines Modellmenüs. Der Survey vom 2026-09-07 belegt die sichtbaren
Beschriftungen. Die Einträge bieten keinen zuverlässigen individuellen
`aria-selected`- oder `aria-pressed`-Marker. Deshalb war die Zelle bisher
`unreachable`.

## Prüfmethode

Der Prüfer bildet für die gesamte Segmentleiste eine Signatur aus Text,
`aria-*`-/`data-*`-Attributen sowie Klassen der Einträge und ihrer Eltern.
Er wählt eine andere Stellung und akzeptiert den Wechsel nur, wenn diese
Signatur sich verändert. Für die Wiederherstellung probiert er ausschließlich
`Instant`, `Expert` und `Vision`, bis die aktuelle Signatur exakt der
ursprünglichen entspricht.

Ein PASS verlangt damit weiterhin alle drei Eigenschaften: beobachtete
Zustandsänderung, ein erreichter abweichender Zustand und exakte Rückkehr zur
Ausgangssignatur in derselben Sitzung. Fehlt eine dieser Eigenschaften, bleibt
die Matrixzelle nicht bestanden; es wird kein Klick als Erfolg gezählt.

## Offene Abnahme

Die Implementierung gehört zu T-502. Erst ein frischer
`verify --brain deepseek --cap model_switch --headless`-Lauf mit dem
vollständigen Vor-/Nach-/Restore-Beleg darf `model/deepseek` von
`unreachable` auf `passed` setzen.
