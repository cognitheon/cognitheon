//! 导出（保存）的统一入口：把字节交付到磁盘（native）或触发浏览器下载（wasm），签名统一为
//! [`trigger_download`]`(filename, bytes, mime)`。
//!
//! ## native
//! `rfd::AsyncFileDialog::save_file` + 传入的 `tokio` runtime `block_on` 写文件——与原 File 菜单
//! Save 行为完全一致。`rfd` / `tokio` 仅在此 `#[cfg(not(wasm32))]` 分支引用，绝不进 wasm 编译
//! （AGENTS.md §2）。
//!
//! ## wasm
//! 同步 DOM 流程（不 `.await`，wasm 下 `block_on` 不可用）：
//! `Uint8Array(bytes)` → `Array.of(u8arr)` → `new Blob([...], {type: mime})`
//! → `URL.createObjectURL(blob)` → 隐藏 `<a download=filename href=url>` → `.click()`
//! → **`URL.revokeObjectURL` 释放**（防 object URL 内存泄漏，§3.6）。

/// 触发一次"导出/下载"：把 `bytes` 以 `filename` 交付（native 落盘 / wasm 浏览器下载）。
///
/// - **native**：弹保存对话框落盘，需 `tokio` runtime 句柄 `block_on` 写文件。
/// - **wasm**：同步触发浏览器下载，无需 runtime（故签名按 cfg 分两版）。
#[cfg(not(target_arch = "wasm32"))]
pub fn trigger_download(
    runtime: &tokio::runtime::Runtime,
    filename: &str,
    bytes: &[u8],
    _mime: &str,
) {
    // 去掉扩展名作为对话框默认文件名（rfd 的 add_filter 已限定 .cnt 扩展）。
    let stem = filename.strip_suffix(".cnt").unwrap_or(filename).to_owned();
    let bytes = bytes.to_vec();
    let future = async move {
        if let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Cognitheon", &["cnt"])
            .set_file_name(&stem)
            .set_directory("~")
            .save_file()
            .await
        {
            match file.write(&bytes).await {
                Ok(_) => log::info!("save success"),
                Err(e) => log::error!("save failed: {e}"),
            }
        }
    };
    runtime.block_on(future);
}

/// 触发一次浏览器下载（wasm）。同步执行整套 DOM 流程，不 `.await`。
#[cfg(target_arch = "wasm32")]
pub fn trigger_download(filename: &str, bytes: &[u8], mime: &str) {
    use wasm_bindgen::JsCast;

    if let Err(e) = trigger_download_impl(filename, bytes, mime) {
        log::error!("download failed: {e:?}");
    }

    // 内部实现：任一步出错即早返回 Err，统一在上面记日志（避免到处 match）。
    fn trigger_download_impl(
        filename: &str,
        bytes: &[u8],
        mime: &str,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let window = web_sys::window().ok_or_else(|| js_err("no window"))?;
        let document = window.document().ok_or_else(|| js_err("no document"))?;

        // bytes → Uint8Array → JS Array([u8arr])（Blob 构造要求 BlobPart 序列）。
        // Uint8Array: From<&[u8]>（js-sys 0.3.102）；Array::of1 包成单元素序列。
        let u8arr = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::of1(&u8arr);

        // Blob 选项：set_type 设 MIME（type_ 已 deprecated，会触发 -D warnings，故用 set_type）。
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type(mime);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts)?;

        // createObjectURL → 必须配对 revokeObjectURL 释放（§3.6 防 object URL 泄漏）。
        let url = web_sys::Url::create_object_url_with_blob(&blob)?;

        // 隐藏 <a download=filename href=url>，模拟点击触发下载。create_element 返回 Element，
        // dyn_into 到 HtmlAnchorElement（web-sys 0.3.102：HtmlAnchorElement extends HtmlElement）。
        let anchor = document
            .create_element("a")?
            .dyn_into::<web_sys::HtmlAnchorElement>()?;
        anchor.set_download(filename);
        anchor.set_href(&url);
        // click() 定义在 HtmlElement 上，HtmlAnchorElement 经 Deref 链可直接调用。
        anchor.click();

        // 立即释放 object URL：下载已由 click 同步发起（浏览器已持有 blob 引用），URL 可安全回收。
        web_sys::Url::revoke_object_url(&url)?;
        log::info!("download triggered: {filename} ({} bytes)", bytes.len());
        Ok(())
    }

    /// 把一段静态错误信息包成 JsValue（仅 wasm 分支内部用）。
    fn js_err(msg: &str) -> wasm_bindgen::JsValue {
        wasm_bindgen::JsValue::from_str(msg)
    }
}
