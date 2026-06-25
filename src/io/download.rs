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

// ============================ Markdown / vault 导出 ============================

/// 触发一次**单文件**导出（Markdown 单节点导出 / wasm vault zip 复用此入口）。与 [`trigger_download`]
/// 同范式但**保留完整文件名**（含 `.md` / `.zip` 扩展，不像 `.cnt` 那样靠 rfd add_filter 钉死扩展）。
///
/// - **native**：弹保存对话框，默认文件名 = 传入的 `filename`，不加扩展名过滤器（任意扩展自由落盘）。
/// - **wasm**：复用 [`trigger_download`] 的 Blob 下载流程（它已不限扩展）。
#[cfg(not(target_arch = "wasm32"))]
pub fn trigger_file_download(
    runtime: &tokio::runtime::Runtime,
    filename: &str,
    bytes: &[u8],
    _mime: &str,
) {
    let filename = filename.to_owned();
    let bytes = bytes.to_vec();
    let future = async move {
        if let Some(file) = rfd::AsyncFileDialog::new()
            .set_file_name(&filename)
            .set_directory("~")
            .save_file()
            .await
        {
            match file.write(&bytes).await {
                Ok(_) => log::info!("export success: {filename}"),
                Err(e) => log::error!("export failed: {e}"),
            }
        }
    };
    runtime.block_on(future);
}

/// wasm 单文件导出：直接复用浏览器下载流程（mime 自定，保留完整 filename）。
#[cfg(target_arch = "wasm32")]
pub fn trigger_file_download(filename: &str, bytes: &[u8], mime: &str) {
    trigger_download(filename, bytes, mime);
}

/// **native vault 导出**：弹目录选择对话框（`rfd::pick_folder`）+ `std::fs` 逐文件写盘。
///
/// 双 target 红线（AGENTS.md §2）：`rfd` / `tokio` / `std::fs` 目录写仅在此 `#[cfg(not(wasm32))]`
/// 分支——wasm 无文件系统，走 [`crate::markdown::vault_zip`] + [`trigger_file_download`] 打包下载
/// （两 target 行为不对称：native 写目录、wasm 下载 zip，属预期）。
///
/// 每个文件的写入失败仅记日志、不中断其余文件（尽力导出）。文件名已由
/// [`crate::markdown::vault_files`] sanitize + 去重，安全拼到所选目录下。
#[cfg(not(target_arch = "wasm32"))]
pub fn trigger_vault_dir_export(
    runtime: &tokio::runtime::Runtime,
    files: &[crate::markdown::VaultFile],
) {
    use std::path::Path;
    let files: Vec<crate::markdown::VaultFile> = files.to_vec();
    let future = async move {
        let Some(dir) = rfd::AsyncFileDialog::new()
            .set_directory("~")
            .pick_folder()
            .await
        else {
            log::info!("vault export cancelled");
            return;
        };
        let dir_path = dir.path().to_path_buf();
        let mut written = 0usize;
        for f in &files {
            // 文件名已 sanitize（无路径分隔符 / 非法字符），安全拼接；用 file_name 兜底防穿越。
            let name = Path::new(&f.filename)
                .file_name()
                .map(|s| s.to_owned())
                .unwrap_or_else(|| f.filename.clone().into());
            let path = dir_path.join(name);
            match std::fs::write(&path, &f.bytes) {
                Ok(_) => written += 1,
                Err(e) => log::error!("vault write failed for {}: {e}", f.filename),
            }
        }
        log::info!(
            "vault exported: {written}/{} files -> {dir_path:?}",
            files.len()
        );
    };
    runtime.block_on(future);
}
