//! 文件进出（Save 导出 / Load 导入）的**统一 IO 网关**——把"native 文件系统"与"web 浏览器
//! 下载/文件选择器"两条彼此不兼容的路径收敛到同一组 API 背后，App 层只面对统一签名、按
//! `#[cfg]` 分派，避免 native / wasm 两套导入导出逻辑各自漂移（AGENTS.md §4.2 禁补丁式分叉）。
//!
//! ## 双 target 红线（AGENTS.md §2）
//! - **native**：`rfd::AsyncFileDialog` + `tokio` `block_on` 写/读文件。`rfd` / `tokio` 的 `net`
//!   特性依赖 `mio`，**绝不能编译进 wasm32**——故它们整体置于 `#[cfg(not(target_arch = "wasm32"))]`，
//!   且只在 native 子模块里被引用。
//! - **wasm**：只用 `web-sys` / `js-sys` / `wasm-bindgen`（已在 `[target.wasm32]` 依赖）经 DOM 触发
//!   下载（`Blob` → `URL.createObjectURL` → `<a download>` 模拟点击 → `revokeObjectURL`）与文件
//!   选择（`<input type=file>` + `FileReader`，异步回调经 egui temp-data 总线回传下一帧消费）。
//!   wasm 下 `block_on` 不可用，导出必须**同步**触发 DOM、不 `.await`。
//!
//! ## 导出（[`download`]）
//! 见 [`download::trigger_download`]：cfg 双分支，签名 `(filename, bytes, mime)` 统一。
//!
//! ## 导入（[`request_open_file`] / [`take_loaded_bytes`]）
//! 导入天生异步（native 的文件对话框 future、wasm 的 `FileReader` 回调），无法在点击那一帧拿到
//! 结果。故统一走"请求 → 下一帧消费"的 **egui temp-data 隐式总线**（与 `FOCUS_REQUEST_KEY` /
//! 上下文菜单请求同范式，§3 隐式状态总线约定）：
//! - [`request_open_file`]：点击 Load 那一帧调用，弹出选择器（native 经 tokio block_on 同步读出后
//!   立即写总线；wasm 经 `FileReader` 异步回调择机写总线并 `request_repaint`）。
//! - [`take_loaded_bytes`]：App 主循环每帧调用一次，**一次性取出并清理**读到的字节（消费即清，
//!   避免重复加载）。

pub mod download;

use egui::Id;

/// 导入文件读出的字节经此 temp-data key 回传给主循环（隐式状态总线，纯运行态、不进序列化）。
///
/// 写入方：native = [`request_open_file`] 内 `block_on` 读出后同帧写入；wasm = `FileReader.onload`
/// 回调里写入（跨帧）。消费方：[`take_loaded_bytes`]（App 每帧一次，取出即移除，保证一份字节只加载一次）。
const LOADED_BYTES_KEY: &str = "io_loaded_file_bytes";

/// 一份待加载的文件字节（被 [`request_open_file`] 写入、[`take_loaded_bytes`] 取走）。
///
/// 用 newtype 包裹 `Vec<u8>` 而非裸 `Vec<u8>`，让它在 egui temp-data 里有唯一类型 key，
/// 不与其它 `Vec<u8>` 用途串味（egui temp-data 按 `(Id, TypeId)` 双键存取）。
#[derive(Clone, Debug, Default)]
pub struct LoadedBytes(pub Vec<u8>);

/// 取出并清理"已读到的待加载文件字节"（一次性消费）。App 主循环每帧调用一次：
/// 读到 `Some(bytes)` 即交给 [`crate::app::CognitheonApp::replace_document`] 替换整图。
///
/// 一次性语义：取出后从 temp-data 移除，**同一份字节只会被加载一次**（避免 FileReader 回调写入后
/// 被后续多帧重复消费）。
pub fn take_loaded_bytes(ctx: &egui::Context) -> Option<Vec<u8>> {
    ctx.data_mut(|d| d.remove_temp::<LoadedBytes>(Id::new(LOADED_BYTES_KEY)))
        .map(|b| b.0)
}

/// 把读到的文件字节写入总线（内部用：native 同帧 / wasm 回调跨帧都经此写）。
fn publish_loaded_bytes(ctx: &egui::Context, bytes: Vec<u8>) {
    ctx.data_mut(|d| d.insert_temp(Id::new(LOADED_BYTES_KEY), LoadedBytes(bytes)));
    // wasm 的 FileReader 回调发生在 egui 帧之外，必须主动请求重绘，否则下一帧可能不被调度、
    // 字节迟迟不被 take_loaded_bytes 消费。native 同帧写入时再请求一次也无害（幂等）。
    ctx.request_repaint();
}

// ============================ native 导入 ============================

/// 弹出文件选择器选取一个 `.cnt`，读出字节后写入 [`LOADED_BYTES_KEY`] 总线（下一帧由
/// [`take_loaded_bytes`] 消费）。
///
/// **native**：`rfd::AsyncFileDialog::pick_file` + 传入的 `tokio` runtime `block_on`——与原 File 菜单
/// Load 行为完全一致（同步读出后立即写总线）。需要 runtime 句柄（App 持有的 native-only 字段）。
#[cfg(not(target_arch = "wasm32"))]
pub fn request_open_file(ctx: &egui::Context, runtime: &tokio::runtime::Runtime) {
    let future = async {
        match rfd::AsyncFileDialog::new()
            .add_filter("Cognitheon", &["cnt"])
            .set_directory("~")
            .pick_file()
            .await
        {
            Some(file) => Some(file.read().await),
            None => None,
        }
    };
    if let Some(bytes) = runtime.block_on(future) {
        publish_loaded_bytes(ctx, bytes);
    }
}

// ============================ wasm 导入 ============================

/// 弹出浏览器文件选择器（`<input type=file accept=".cnt">`）选取文件，经 `FileReader` 异步读出
/// 字节后写入总线。
///
/// **wasm**：DOM 操作同步触发（创建隐藏 `<input>` 并 `.click()`），但读取是 `FileReader` 的
/// **异步回调**——回调里把字节写入 temp-data 总线并 `request_repaint`，下一帧 [`take_loaded_bytes`]
/// 消费。`Closure` 生命周期经 `forget()` 交给 JS GC（一次性导入，悬垂窗口极短、不构成实际泄漏；
/// 详见函数体注释）。
#[cfg(target_arch = "wasm32")]
pub fn request_open_file(ctx: &egui::Context) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let Some(window) = web_sys::window() else {
        log::error!("import: no window");
        return;
    };
    let Some(document) = window.document() else {
        log::error!("import: no document");
        return;
    };

    // 创建隐藏的 <input type=file accept=".cnt">。create_element 返回 Element，dyn_into 到
    // HtmlInputElement（web-sys 0.3.102：HtmlInputElement extends HtmlElement/Element）。
    let input = match document.create_element("input") {
        Ok(el) => match el.dyn_into::<web_sys::HtmlInputElement>() {
            Ok(input) => input,
            Err(_) => {
                log::error!("import: <input> not an HtmlInputElement");
                return;
            }
        },
        Err(e) => {
            log::error!("import: create <input> failed: {e:?}");
            return;
        }
    };
    input.set_type("file");
    input.set_accept(".cnt");

    let ctx = ctx.clone();
    let input_for_cb = input.clone();
    // onchange：用户选好文件后触发。读出 FileList 第一个 File，用 FileReader 异步读 ArrayBuffer。
    let onchange = Closure::<dyn FnMut()>::new(move || {
        let Some(files) = input_for_cb.files() else {
            return;
        };
        let Some(file) = files.get(0) else {
            return;
        };

        let reader = match web_sys::FileReader::new() {
            Ok(r) => r,
            Err(e) => {
                log::error!("import: FileReader::new failed: {e:?}");
                return;
            }
        };

        let ctx_inner = ctx.clone();
        let reader_for_onload = reader.clone();
        // onload：读取完成后从 reader.result() 取 ArrayBuffer → Uint8Array → Vec<u8> → 写总线。
        let onload = Closure::<dyn FnMut()>::new(move || {
            match reader_for_onload.result() {
                Ok(buffer) => {
                    // result 是 ArrayBuffer（read_as_array_buffer），用 Uint8Array::new 视图后 to_vec。
                    let array = js_sys::Uint8Array::new(&buffer);
                    let bytes = array.to_vec();
                    log::info!("import: read {} bytes", bytes.len());
                    publish_loaded_bytes(&ctx_inner, bytes);
                }
                Err(e) => log::error!("import: FileReader result error: {e:?}"),
            }
        });
        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
        // 一次性导入：把 onload Closure 交给 JS（forget）。它需活到异步 onload 触发，无法在此处
        // drop；导入是低频一次性操作，泄漏量极小且有界（每次导入一个小闭包），可接受。
        onload.forget();

        if let Err(e) = reader.read_as_array_buffer(&file) {
            log::error!("import: read_as_array_buffer failed: {e:?}");
        }
    });
    input.set_onchange(Some(onchange.as_ref().unchecked_ref()));
    // 同理：onchange 需活到用户选择文件后触发，forget 交给 JS。
    onchange.forget();

    // 触发原生文件选择对话框（同步，不 await）。input 不挂进 DOM 也能 click() 弹出选择器。
    input.click();
}
