use super::{cdp_click_events, parse_eval_result, press_key_script, wrap_eval};

// Der vertrauenswuerdige Klick ist eine Pointer-Sequenz
// (pressed -> released) mit Koordinaten, button und clickCount —
// nicht ein einziges synthetisches DOM-Event. Genau daran haengt, ob
// qwens Denkstufen-Menue aufpoppt: die Oberflaeche lauscht auf trusted
// pointerdown, und das liefert nur `Input.dispatchMouseEvent` ueber CDP.
#[test]
fn cdp_klick_sendet_vollstaendige_pointer_sequenz() {
    let events = cdp_click_events(123.0, 456.0);
    let types: Vec<&str> = events.iter().map(|(t, _)| *t).collect();
    assert_eq!(types, vec!["mousePressed", "mouseReleased"]);
    for (t, params) in &events {
        assert!(
            params.contains("\"x\":123") && params.contains("\"y\":456"),
            "{t} muss die Koordinaten tragen: {params}"
        );
        assert!(params.contains("\"button\":\"left\""), "{t}: {params}");
    }
    assert!(
        events[0].1.contains("\"clickCount\":1"),
        "pressed braucht clickCount: {}",
        events[0].1
    );
}

// Die Bedingung, unter der die ganze Kachelansicht steht: Enter absenden
// darf nicht am Betriebssystem-Fokus haengen. Tut es auch nicht — der
// Tastendruck ist ein DOM-Event an `document.activeElement`, das ueber
// `evaluate_script` in die Seite gereicht wird. Deshalb duerfen die
// Kachelfenster `WS_EX_NOACTIVATE` tragen, ohne den Absendeweg zu beruehren.
// Kaeme hier je ein Betriebssystem-Weg (SendInput, keybd_event, SendMessage
// an das Fenster) hinein, waere diese Annahme gebrochen — der Test schlaegt
// dann an, bevor der Relay stillschweigend in Timeouts laeuft.
#[test]
fn enter_wird_im_dom_ausgeloest_nicht_ueber_den_fensterfokus() {
    let js = press_key_script("Enter", "Enter", 13, "\r");
    assert!(
        js.contains("document.activeElement"),
        "Enter muss ans fokussierte DOM-Element gehen: {js}"
    );
    assert!(
        js.contains("new KeyboardEvent('keydown'"),
        "Enter muss ein DOM-KeyboardEvent sein: {js}"
    );
    for os_weg in ["SendInput", "keybd_event", "PostMessage", "SendMessage"] {
        assert!(
            !js.contains(os_weg),
            "{os_weg} wuerde den Absendeweg an den Fensterfokus binden: {js}"
        );
    }
    assert!(js.contains("13"), "keyCode fehlt: {js}");
}

// Regression: der Wrapper darf kein Promise und keinen String liefern.
// WebView2 serialisiert das Skript-Ergebnis; eine async-IIFE kommt als "{}"
// zurueck und JSON.stringify als doppelt kodierter String. Beides liess
// jede DOM-Abfrage leer aussehen (logged_in=false fuer alle Provider).
#[test]
fn wrap_eval_is_sync_and_returns_object() {
    let js = wrap_eval("1+1");
    assert!(
        !js.contains("async"),
        "async-IIFE liefert ein Promise: {js}"
    );
    assert!(!js.contains("await"), "await erzwingt ein Promise: {js}");
    assert!(
        !js.contains("JSON.stringify"),
        "stringify erzeugt doppelt kodierten String: {js}"
    );
    assert!(js.contains("ok:true"), "Erfolgs-Huelle fehlt: {js}");
    assert!(js.contains("(1+1)"), "Ausdruck nicht eingebettet: {js}");
}

#[test]
fn parse_eval_result_unwraps_ok_value() {
    let v = parse_eval_result(r#"{"ok":true,"value":2}"#.to_string()).unwrap();
    assert_eq!(v, serde_json::json!(2));
}

#[test]
fn parse_eval_result_maps_js_exception_to_err() {
    let e = parse_eval_result(r#"{"ok":false,"error":"ReferenceError: x"}"#.to_string());
    assert!(e.is_err(), "JS-Ausnahme muss Err werden");
}

// Das war das reale Symptom: der async-Wrapper lieferte wortwoertlich "{}".
#[test]
fn parse_eval_result_on_empty_object_yields_no_value() {
    let v = parse_eval_result("{}".to_string()).unwrap();
    assert!(
        v.get("value").is_none(),
        "leeres Objekt darf keinen Wert vortaeuschen"
    );
}
