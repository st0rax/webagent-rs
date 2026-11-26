//! Pure SPA/CDP upload JS snippets and helpers (no WebView deps).
//! Kept outside `webview` so Linux `--no-default-features` unit tests cover them.

/// SPA notify body for `Runtime.callFunctionOn` on the chooser file input.
/// Dispatches bubbling `input` + `change` (and InputEvent when available) and
/// reports `files.length` on THAT node — not via `querySelector`.
pub(crate) fn spa_file_input_notify_fn() -> &'static str {
    r#"function() {
      var before = (this.files && this.files.length) || 0;
      var connected = this.isConnected !== false;
      var events = false;
      var err = null;
      try {
        this.dispatchEvent(new Event('input', {bubbles:true, cancelable:true}));
        this.dispatchEvent(new Event('change', {bubbles:true, cancelable:true}));
        try {
          this.dispatchEvent(new InputEvent('input', {bubbles:true, cancelable:true}));
        } catch (e0) {}
        events = true;
      } catch (e) { err = String(e); }
      var after = (this.files && this.files.length) || 0;
      return {
        filesBefore: before,
        filesAfter: after,
        events: events,
        connected: connected,
        error: err
      };
    }"#
}

/// Last-resort Vue/React FileList override when CDP `setFileInputFiles` left
/// `files.length === 0` on the chooser node. Only used after a trusted set.
pub(crate) fn spa_filelist_hack_fn() -> &'static str {
    r#"function(payloads) {
      try {
        var dt = new DataTransfer();
        for (var i = 0; i < payloads.length; i++) {
          var p = payloads[i];
          var bin = atob(p.b64);
          var bytes = new Uint8Array(bin.length);
          for (var j = 0; j < bin.length; j++) bytes[j] = bin.charCodeAt(j);
          dt.items.add(new File([bytes], p.name, {type: p.mime || 'application/octet-stream'}));
        }
        try {
          Object.defineProperty(this, 'files', {
            configurable: true,
            enumerable: true,
            get: function() { return dt.files; }
          });
        } catch (e1) {
          try { this.files = dt.files; } catch (e2) {}
        }
        this.dispatchEvent(new Event('input', {bubbles:true, cancelable:true}));
        this.dispatchEvent(new Event('change', {bubbles:true, cancelable:true}));
        return {ok:true, files: (this.files && this.files.length) || 0, hacked:true};
      } catch (e) {
        return {ok:false, files: (this.files && this.files.length) || 0, error: String(e)};
      }
    }"#
}

/// Prefer dedicated attachment drop overlays (qwen/zai) before composer center.
pub(crate) fn drag_drop_target_expression() -> &'static str {
    r#"(() => {
      const visible = (el) => {
        if (!el) return false;
        const r = el.getBoundingClientRect(), s = getComputedStyle(el);
        return r.width > 0 && r.height > 0 && s.display !== 'none' && s.visibility !== 'hidden';
      };
      const pick = (el, via) => {
        const r = el.getBoundingClientRect();
        return {x: r.left + r.width / 2, y: r.top + r.height / 2, via: via, w: r.width, h: r.height};
      };
      const dropSels = [
        '[class*="dropzone" i]','[class*="drop-zone" i]','[class*="drop-area" i]',
        '[data-dropzone]','[class*="upload-area" i]','[class*="upload-zone" i]',
        '[class*="drag-over" i]','[class*="file-drop" i]',
        '[class*="attachment-area" i]','[class*="attachments" i]',
        '[class*="chat-input" i] [class*="drop" i]'
      ];
      for (const sel of dropSels) {
        for (const el of document.querySelectorAll(sel)) {
          if (visible(el)) return pick(el, 'dropzone:' + sel);
        }
      }
      const composerSels = [
        '[class*="message-input" i]','[class*="composer" i]','#chat-input',
        'textarea.message-input-textarea','[contenteditable="true"]','textarea','[role="textbox"]'
      ];
      for (const sel of composerSels) {
        for (const el of document.querySelectorAll(sel)) {
          if (visible(el)) return pick(el, 'composer:' + sel);
        }
      }
      return null;
    })()"#
}

pub(crate) fn b64_encode_upload(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(TABLE[((n >> 6) & 63) as usize] as char);
        out.push(TABLE[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let n = (data[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(TABLE[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        b64_encode_upload, drag_drop_target_expression, spa_file_input_notify_fn,
        spa_filelist_hack_fn,
    };

    #[test]
    fn spa_file_input_notify_dispatches_input_and_change() {
        let js = spa_file_input_notify_fn();
        assert!(js.contains("dispatchEvent(new Event('input'"), "{js}");
        assert!(js.contains("dispatchEvent(new Event('change'"), "{js}");
        assert!(
            js.contains("filesBefore") && js.contains("filesAfter"),
            "{js}"
        );
        assert!(js.contains("bubbles:true"), "{js}");
    }

    #[test]
    fn spa_filelist_hack_defines_files_and_fires_change() {
        let js = spa_filelist_hack_fn();
        assert!(js.contains("Object.defineProperty(this, 'files'"), "{js}");
        assert!(js.contains("DataTransfer"), "{js}");
        assert!(js.contains("dispatchEvent(new Event('change'"), "{js}");
    }

    #[test]
    fn drag_drop_target_prefers_dropzone_before_composer() {
        let js = drag_drop_target_expression();
        assert!(js.contains("dropzone") || js.contains("drop-zone"), "{js}");
        let drop_pos = js.find("dropSels").expect("dropSels");
        let composer_pos = js.find("composerSels").expect("composerSels");
        assert!(
            drop_pos < composer_pos,
            "drop overlay selectors must be tried before composer center"
        );
    }

    #[test]
    fn b64_encode_upload_roundtrips_known_vector() {
        assert_eq!(b64_encode_upload(b""), "");
        assert_eq!(b64_encode_upload(b"f"), "Zg==");
        assert_eq!(b64_encode_upload(b"fo"), "Zm8=");
        assert_eq!(b64_encode_upload(b"foo"), "Zm9v");
        assert_eq!(b64_encode_upload(&[0x01, 0x02]), "AQI=");
    }
}
