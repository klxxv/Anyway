use std::alloc::{self, Layout};
use std::mem;

/// Host-rendered setting kinds. `Secret` is write-only host state: the guest
/// SDK intentionally has no getter for its plaintext value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginSettingType {
    Boolean,
    Number,
    Text,
    Select,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginApiFormat {
    OpenAi,
    Anthropic,
}

#[derive(Clone, Copy, Debug)]
pub enum PluginApiKeySource<'a> {
    HostSecret { setting_id: &'a str },
    Environment { name: &'a str, fallback_setting_id: Option<&'a str> },
}

#[derive(Clone, Copy, Debug)]
pub enum PluginConnectionTestActionInput<'a> {
    Text,
    BundledPdf {
        fixture: &'a str,
        file_upload: &'a str,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct PluginConnectionTestAction<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub label_key: Option<&'a str>,
    pub description: Option<&'a str>,
    pub description_key: Option<&'a str>,
    pub placeholder: Option<&'a str>,
    pub placeholder_key: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub input: Option<PluginConnectionTestActionInput<'a>>,
}

/// Declarative connection metadata rendered and tested by the native host.
#[derive(Clone, Copy, Debug)]
pub struct PluginConnectionDefinition<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub label_key: Option<&'a str>,
    pub description: Option<&'a str>,
    pub description_key: Option<&'a str>,
    pub placeholder: Option<&'a str>,
    pub placeholder_key: Option<&'a str>,
    pub url_setting_id: &'a str,
    pub format_setting_id: &'a str,
    pub model_setting_id: Option<&'a str>,
    pub credential_source_setting_id: Option<&'a str>,
    pub credential_env_var_setting_id: Option<&'a str>,
    pub api_key: PluginApiKeySource<'a>,
    pub test_actions: &'a [PluginConnectionTestAction<'a>],
    /// Legacy single-action spelling for older manifests.
    pub test_action_id: Option<&'a str>,
}

/// Stable plugin identity metadata. `developer_id` is optional for legacy
/// manifests and should contain a UUID when present.
#[derive(Clone, Copy, Debug)]
pub struct PluginIdentity<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub developer: &'a str,
    pub developer_id: Option<&'a str>,
}

#[derive(Clone, Copy, Debug)]
pub struct PluginSettingOption<'a> {
    pub value: &'a str,
    pub label: &'a str,
    pub label_key: Option<&'a str>,
    pub description: Option<&'a str>,
    pub description_key: Option<&'a str>,
    pub placeholder: Option<&'a str>,
    pub placeholder_key: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PluginSettingDefault<'a> {
    Boolean(bool),
    Number(f64),
    Text(&'a str),
}

/// A lightweight declaration shape matching the `plugin.yml` JSON/YAML keys.
/// The host owns validation, persistence, and UI rendering.
#[derive(Clone, Copy, Debug)]
pub struct PluginSettingDefinition<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub label_key: Option<&'a str>,
    pub setting_type: PluginSettingType,
    pub secret: bool,
    pub required: bool,
    pub description: Option<&'a str>,
    pub description_key: Option<&'a str>,
    pub placeholder: Option<&'a str>,
    pub placeholder_key: Option<&'a str>,
    pub group: Option<&'a str>,
    pub default: Option<PluginSettingDefault<'a>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub options: &'a [PluginSettingOption<'a>],
}

/// Plugin-owned messages are loaded by the host from locales/<tag>.json and
/// are never registered in the host's global LocalePlugin catalog.
#[derive(Clone, Copy, Debug)]
pub struct PluginPrivateI18n<'a> {
    pub default_locale: &'a str,
    pub locales: &'a [PluginPrivateLocale<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct PluginPrivateLocale<'a> {
    pub locale: &'a str,
    pub path: &'a str,
}

/// Values already validated and resolved by the host. Secret values are not a
/// variant by design; model credentials stay inside the host gateway.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingValue<'a> {
    Boolean(bool),
    Number(f64),
    Text(&'a str),
}

/// Host-provided read contract for effective, non-secret settings.
pub trait SettingReader {
    fn get(&self, id: &str) -> Option<SettingValue<'_>>;

    fn get_boolean(&self, id: &str) -> Option<bool> {
        match self.get(id) {
            Some(SettingValue::Boolean(value)) => Some(value),
            _ => None,
        }
    }

    fn get_number(&self, id: &str) -> Option<f64> {
        match self.get(id) {
            Some(SettingValue::Number(value)) => Some(value),
            _ => None,
        }
    }

    fn get_text(&self, id: &str) -> Option<&str> {
        match self.get(id) {
            Some(SettingValue::Text(value)) => Some(value),
            _ => None,
        }
    }
}

/// 分配 host 可读的 guest 内存；返回的指针必须由宿主通过 `myc_free` 释放。
/// Allocates host-readable guest memory; the returned pointer must be freed by
/// the host using `myc_free`.
#[no_mangle]
pub extern "C" fn myc_alloc(size: i32) -> i32 {
    let size = size.max(0) as usize;
    if size == 0 {
        return 0;
    }
    let layout = Layout::from_size_align(size, 1).expect("valid layout");
    let pointer = unsafe { alloc::alloc(layout) };
    if pointer.is_null() {
        return 0;
    }
    pointer as i32
}

/// 释放由 `myc_alloc` 返回的 guest 内存；size 必须与分配时一致。
/// Frees memory returned by `myc_alloc`; `size` must match the allocation.
#[no_mangle]
pub extern "C" fn myc_free(pointer: i32, size: i32) {
    let pointer = pointer as *mut u8;
    let size = size.max(0) as usize;
    if pointer.is_null() || size == 0 {
        return;
    }
    let layout = Layout::from_size_align(size, 1).expect("valid layout");
    unsafe { alloc::dealloc(pointer, layout) };
}

#[no_mangle]
pub extern "C" fn myc_run(_input_pointer: i32, _input_length: i32) -> i64 {
    let output = br#"{"runtime":"rust","status":"ok"}"#.to_vec().into_boxed_slice();
    let length = output.len() as u64;
    let pointer = output.as_ptr() as u64;
    mem::forget(output);
    ((pointer & 0xffff_ffff) << 32 | length) as i64
}
