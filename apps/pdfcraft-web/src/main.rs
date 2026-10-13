//! PdfCraft in the browser. Build: `cd apps/pdfcraft-web && trunk build --release`
//! (trunk generates the JS loader; no handwritten JS — plan/execution-plan.md §1).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast;
    use pdfcraft_ui_egui::PdfCraftApp;

    let options = eframe::WebOptions { renderer: eframe::Renderer::Glow, ..Default::default() };
    wasm_bindgen_futures::spawn_local(async move {
        let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("pdfcraft"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            return;
        };
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| {
                    let mut app = PdfCraftApp::new();
                    // "printcraft": settings saved under the app's former name, PrintCraft.
                    if let Some(json) = cc.storage.and_then(|s| s.get_string("pdfcraft").or_else(|| s.get_string("printcraft"))) {
                        app.restore(&json);
                    }
                    // `?file=<url>` opens a PDF from a URL (same-origin or CORS-enabled).
                    if let Some(url) = query_param("file") {
                        let inbox = app.inbox.clone();
                        let ctx = cc.egui_ctx.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            match fetch_bytes(&url).await {
                                Ok(bytes) => {
                                    let name =
                                        url.rsplit('/').next().unwrap_or("document.pdf").split('?').next().unwrap_or("document.pdf").to_string();
                                    if let Ok(mut q) = inbox.lock() {
                                        q.push(pdfcraft_ui_egui::IncomingFile::local(name, bytes));
                                    }
                                    ctx.request_repaint();
                                }
                                Err(e) => eframe::web_sys::console::error_1(&format!("PdfCraft: could not fetch {url}: {e}").into()),
                            }
                        });
                    }
                    install_parent_bridge(&mut app, cc.egui_ctx.clone());
                    Ok(Box::new(app))
                }),
            )
            .await;
        if let Err(e) = started {
            eframe::web_sys::console::error_1(&e);
        }
    });
}

/// Let a containing RiRead page open local PDFs without rebuilding the existing reader UI.
///
/// The parent opts in by adding `?parentOrigin=<its origin>` to the iframe URL, then sends:
/// `{ type: "riread:open", documentId, name, bytes: ArrayBuffer }`.
#[cfg(target_arch = "wasm32")]
fn install_parent_bridge(app: &mut pdfcraft_ui_egui::PdfCraftApp, ctx: egui::Context) {
    use eframe::wasm_bindgen::{JsCast, closure::Closure};

    let Some(parent_origin) = query_param("parentOrigin").filter(|origin| origin != "null" && origin != "*") else {
        return;
    };
    let Some(window) = web_sys::window() else { return };
    let inbox = app.inbox.clone();
    let save_results = app.external_save_results();
    let handler_origin = parent_origin.clone();
    let handler = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
        if event.origin() != handler_origin {
            return;
        }
        let data = event.data();
        match js_string(&data, "type").as_deref() {
            Some("riread:open") => {
                let Some(bytes) = js_value(&data, "bytes") else {
                    post_parent(&handler_origin, "riread:error", Some("The open message did not include PDF bytes."));
                    return;
                };
                let name = js_string(&data, "name").filter(|name| !name.trim().is_empty()).unwrap_or_else(|| "document.pdf".to_string());
                let bytes = js_sys::Uint8Array::new(&bytes).to_vec();
                if bytes.is_empty() {
                    post_parent(&handler_origin, "riread:error", Some("The selected PDF is empty."));
                    return;
                }
                let file = match js_string(&data, "documentId").filter(|id| !id.trim().is_empty()) {
                    Some(document_id) => pdfcraft_ui_egui::IncomingFile::stored(name, bytes, document_id),
                    None => pdfcraft_ui_egui::IncomingFile::local(name, bytes),
                };
                let queued = inbox.lock().map(|mut queue| queue.push(file)).is_ok();
                if !queued {
                    post_parent(&handler_origin, "riread:error", Some("The reader could not accept the PDF."));
                    return;
                }
                ctx.request_repaint();
                post_parent(&handler_origin, "riread:open-queued", None);
            }
            Some("riread:save-result") => {
                let Some(request_id) = js_string(&data, "requestId").filter(|id| !id.trim().is_empty()) else { return };
                let Some(ok) = js_bool(&data, "ok") else { return };
                let result = pdfcraft_ui_egui::ExternalSaveResult { request_id, ok, error: js_string(&data, "error") };
                if let Ok(mut queue) = save_results.lock() {
                    queue.push(result);
                    ctx.request_repaint();
                }
            }
            _ => {}
        }
    });
    if window.add_event_listener_with_callback("message", handler.as_ref().unchecked_ref()).is_ok() {
        let save_origin = parent_origin.clone();
        app.set_external_save_sender(Box::new(move |request| post_save_request(&save_origin, &request)));
        handler.forget();
        post_parent(&parent_origin, "riread:ready", None);
    }
}

#[cfg(target_arch = "wasm32")]
fn js_value(object: &eframe::wasm_bindgen::JsValue, key: &str) -> Option<eframe::wasm_bindgen::JsValue> {
    js_sys::Reflect::get(object, &key.into()).ok().filter(|value| !value.is_null() && !value.is_undefined())
}

#[cfg(target_arch = "wasm32")]
fn js_string(object: &eframe::wasm_bindgen::JsValue, key: &str) -> Option<String> {
    js_value(object, key).and_then(|value| value.as_string())
}

#[cfg(target_arch = "wasm32")]
fn js_bool(object: &eframe::wasm_bindgen::JsValue, key: &str) -> Option<bool> {
    js_value(object, key).and_then(|value| value.as_bool())
}

#[cfg(target_arch = "wasm32")]
fn post_save_request(origin: &str, request: &pdfcraft_ui_egui::ExternalSaveRequest) -> Result<(), String> {
    let window = web_sys::window().ok_or("no browser window")?;
    let parent = window.parent().map_err(|e| format!("{e:?}"))?.ok_or("no parent window")?;
    let payload = js_sys::Object::new();
    set_property(&payload, "type", &"riread:save-request".into())?;
    set_property(&payload, "requestId", &request.request_id.clone().into())?;
    set_property(&payload, "documentId", &request.document_id.clone().into())?;
    set_property(&payload, "name", &request.name.clone().into())?;
    let array = js_sys::Uint8Array::from(request.bytes.as_slice());
    let buffer = array.buffer();
    set_property(&payload, "bytes", &buffer)?;
    // Copy the snapshot across the iframe boundary. Transferring the WebAssembly-created buffer
    // would save one copy, but a normal structured clone is more reliable across browser realms
    // for this local-first v1 flow.
    parent.post_message(&payload, origin).map_err(|e| format!("{e:?}"))
}

#[cfg(target_arch = "wasm32")]
fn set_property(object: &js_sys::Object, key: &str, value: &eframe::wasm_bindgen::JsValue) -> Result<(), String> {
    match js_sys::Reflect::set(object, &key.into(), value) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("could not set bridge field {key}")),
        Err(e) => Err(format!("{e:?}")),
    }
}

#[cfg(target_arch = "wasm32")]
fn post_parent(origin: &str, kind: &str, message: Option<&str>) {
    let Some(window) = web_sys::window() else { return };
    let Ok(Some(parent)) = window.parent() else { return };
    let payload = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&payload, &"type".into(), &kind.into());
    if let Some(message) = message {
        let _ = js_sys::Reflect::set(&payload, &"message".into(), &message.into());
    }
    let _ = parent.post_message(&payload, origin);
}

#[cfg(target_arch = "wasm32")]
fn query_param(key: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search).ok()?.get(key)
}

#[cfg(target_arch = "wasm32")]
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    use eframe::wasm_bindgen::JsCast;
    let window = web_sys::window().ok_or("no window")?;
    let resp = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url)).await.map_err(|e| format!("{e:?}"))?;
    let resp: web_sys::Response = resp.dyn_into().map_err(|_| "not a response")?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let buf = wasm_bindgen_futures::JsFuture::from(resp.array_buffer().map_err(|e| format!("{e:?}"))?).await.map_err(|e| format!("{e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("pdfcraft-web targets wasm32: run `trunk serve` or `trunk build --release` in apps/pdfcraft-web");
}
