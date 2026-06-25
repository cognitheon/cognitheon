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

// ============================ Markdown 多文件导入总线 ============================

/// 一批待导入的 Markdown vault 文件（文件名 + 字节）经此 temp-data key 回传给主循环。
///
/// 与 [`LOADED_BYTES_KEY`]（单文件 `.cnt` 替换整图）刻意分开：vault 导入是**追加到既有图**、
/// 多文件、需要文件名（→ `Node.text`）。消费方 [`take_loaded_md_files`]（App 每帧一次，取走即清）。
const LOADED_MD_FILES_KEY: &str = "io_loaded_md_files";

/// 一批待导入的 Markdown 文件（`Vec<(filename, bytes)>`）。
///
/// newtype 包裹以在 egui temp-data 里有唯一类型 key（按 `(Id, TypeId)` 双键存取），不与单文件
/// [`LoadedBytes`] 或任何 `Vec<...>` 用途串味。
#[derive(Clone, Debug, Default)]
pub struct LoadedMdFiles(pub Vec<(String, Vec<u8>)>);

/// 取出并清理"已读到的一批 Markdown 文件"（一次性消费）。App 主循环每帧调用一次：
/// 读到 `Some(files)` 即交给 [`crate::app::CognitheonApp`] 的导入流水线（追加进既有图、resolve 连边）。
///
/// 一次性语义：取出后从 temp-data 移除，同一批文件只导入一次（避免 FileReader 多回调聚合后被多帧重复消费）。
pub fn take_loaded_md_files(ctx: &egui::Context) -> Option<Vec<(String, Vec<u8>)>> {
    ctx.data_mut(|d| d.remove_temp::<LoadedMdFiles>(Id::new(LOADED_MD_FILES_KEY)))
        .map(|f| f.0)
        .filter(|v| !v.is_empty())
}

/// 把一批读到的 Markdown 文件写入总线（内部用）。native 一次性写齐；wasm 由各 FileReader 回调
/// **累加**进同一批（见 [`request_open_markdown_files`] 的 wasm 实现，按已读计数判定整批读完后才落总线）。
fn publish_loaded_md_files(ctx: &egui::Context, files: Vec<(String, Vec<u8>)>) {
    ctx.data_mut(|d| d.insert_temp(Id::new(LOADED_MD_FILES_KEY), LoadedMdFiles(files)));
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

/// **native**：弹**多选**文件对话框选取若干 `.md` / `.json`（手画边旁路），逐个读出字节后整批写入
/// [`LOADED_MD_FILES_KEY`] 总线（下一帧由 [`take_loaded_md_files`] 消费 → 导入流水线）。
///
/// `rfd::pick_files`（多选）+ `tokio` `block_on` 同步读出——与单文件 Load 同范式，只是多文件 + 不同
/// 过滤器 + 不同总线。文件名取 `file_name()` 末段（→ `Node.text`），任一文件读失败仅记日志、跳过。
#[cfg(not(target_arch = "wasm32"))]
pub fn request_open_markdown_files(ctx: &egui::Context, runtime: &tokio::runtime::Runtime) {
    let future = async {
        let Some(handles) = rfd::AsyncFileDialog::new()
            .add_filter("Markdown / vault", &["md", "json"])
            .set_directory("~")
            .pick_files()
            .await
        else {
            return Vec::new();
        };
        let mut out: Vec<(String, Vec<u8>)> = Vec::with_capacity(handles.len());
        for h in handles {
            // file_name() 是末段文件名（不含路径），正是导入侧要的 `标题.md` / 旁路名。
            out.push((h.file_name(), h.read().await));
        }
        out
    };
    let files = runtime.block_on(future);
    if !files.is_empty() {
        publish_loaded_md_files(ctx, files);
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

/// **wasm**：弹**多选**文件选择器（`<input type=file multiple accept=".md,.json">`）选取若干文件，
/// 为每个 `File` 起一个 `FileReader` 异步读 `ArrayBuffer`，**累加**进一个共享缓冲区；待**全部**读完
/// （成功或失败计入"已结算"计数）才整批写入 [`LOADED_MD_FILES_KEY`] 总线（下一帧 [`take_loaded_md_files`]
/// 消费 → 导入流水线）。
///
/// 为什么累加后整批落总线：导入流水线对顺序敏感（先全建点再 resolve，AGENTS.md §3.3），且占位坐标
/// 缺失时整批跑一次力导向布局——必须以"完整一批"为单位进入，不能各文件读完就各自 import。
///
/// `Closure` / 共享缓冲区生命周期（仿单文件 [`request_open_file`]）：onchange 与各 onload 闭包都
/// `forget()` 交给 JS GC；共享缓冲区 `Rc<RefCell<…>>` 被各 onload 闭包按引用捕获，最后一个回调触发
/// 落总线后引用自然清零。一次性低频操作，悬垂窗口极短、不构成实际泄漏（与单文件同口径）。
#[cfg(target_arch = "wasm32")]
pub fn request_open_markdown_files(ctx: &egui::Context) {
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let Some(window) = web_sys::window() else {
        log::error!("import md: no window");
        return;
    };
    let Some(document) = window.document() else {
        log::error!("import md: no document");
        return;
    };

    let input = match document.create_element("input") {
        Ok(el) => match el.dyn_into::<web_sys::HtmlInputElement>() {
            Ok(input) => input,
            Err(_) => {
                log::error!("import md: <input> not an HtmlInputElement");
                return;
            }
        },
        Err(e) => {
            log::error!("import md: create <input> failed: {e:?}");
            return;
        }
    };
    input.set_type("file");
    input.set_accept(".md,.json");
    input.set_multiple(true);

    let ctx = ctx.clone();
    let input_for_cb = input.clone();
    let onchange = Closure::<dyn FnMut()>::new(move || {
        let Some(files) = input_for_cb.files() else {
            return;
        };
        let total = files.length() as usize;
        if total == 0 {
            return;
        }

        // 共享累加缓冲：collected = 已读出的 (filename, bytes)；settled = 已结算（成功/失败）文件数。
        // 全部结算后整批落总线（以"完整一批"为单位进入顺序敏感的导入流水线）。
        let collected: Rc<RefCell<Vec<(String, Vec<u8>)>>> =
            Rc::new(RefCell::new(Vec::with_capacity(total)));
        let settled: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));

        for i in 0..total {
            let Some(file) = files.get(i as u32) else {
                // 取不到也算一次结算，避免整批永远凑不齐 total。
                *settled.borrow_mut() += 1;
                continue;
            };
            let filename = file.name(); // 末段文件名（→ Node.text / 旁路识别）。
            let reader = match web_sys::FileReader::new() {
                Ok(r) => r,
                Err(e) => {
                    log::error!("import md: FileReader::new failed: {e:?}");
                    *settled.borrow_mut() += 1;
                    continue;
                }
            };

            let ctx_inner = ctx.clone();
            let collected_cb = collected.clone();
            let settled_cb = settled.clone();
            let reader_for_onload = reader.clone();
            let onload = Closure::<dyn FnMut()>::new(move || {
                match reader_for_onload.result() {
                    Ok(buffer) => {
                        let array = js_sys::Uint8Array::new(&buffer);
                        let bytes = array.to_vec();
                        collected_cb.borrow_mut().push((filename.clone(), bytes));
                    }
                    Err(e) => {
                        log::error!("import md: FileReader result error for {filename}: {e:?}")
                    }
                }
                // 无论成功/失败都结算一次；最后一个结算者负责整批落总线。
                let done = {
                    let mut s = settled_cb.borrow_mut();
                    *s += 1;
                    *s >= total
                };
                if done {
                    let batch = std::mem::take(&mut *collected_cb.borrow_mut());
                    log::info!("import md: read {} files", batch.len());
                    publish_loaded_md_files(&ctx_inner, batch);
                }
            });
            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
            onload.forget();

            if let Err(e) = reader.read_as_array_buffer(&file) {
                log::error!(
                    "import md: read_as_array_buffer failed for {}: {e:?}",
                    file.name()
                );
                // 读启动失败也要结算，避免整批凑不齐。
                let done = {
                    let mut s = settled.borrow_mut();
                    *s += 1;
                    *s >= total
                };
                if done {
                    let batch = std::mem::take(&mut *collected.borrow_mut());
                    publish_loaded_md_files(&ctx, batch);
                }
            }
        }
    });
    input.set_onchange(Some(onchange.as_ref().unchecked_ref()));
    onchange.forget();

    input.click();
}
