#![allow(dead_code)] // Non-Windows stubs + helpers only used by either cfg arm.

//! Windows COM (IDispatch) bridge.
//!
//! Enabled via the `com` feature. On non-Windows targets every entry point
//! returns a "not supported" error so scripts still compile and run — they
//! just fail at the point they try to touch a COM object. On Windows, this
//! wraps `IDispatch` late-binding (the same mechanism VBScript uses).
//!
//! Values cross the boundary as wscript's usual i32 / str handles. A
//! thread-local "last error" slot lets scripts retrieve a formatted error
//! message after any fallible `com_*` call returns a sentinel.
//!
//! Note: the Windows invoke path uses the low-level IDispatch::Invoke API
//! from the `windows` crate. The exact VARIANT construction is sensitive to
//! crate version; this module is driven by the wine integration tests in
//! `crates/wscript-com-test` — if a version bump breaks the build, iterate
//! from there.

use std::sync::Mutex;

// Process-global last-error slot. We use a Mutex rather than a thread_local
// because wasmtime with the `async` feature may run host closures on
// threads distinct from the one set the error on.
static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

pub(crate) fn set_last_error(msg: impl Into<String>) {
    if let Ok(mut guard) = LAST_ERROR.lock() {
        *guard = msg.into();
    }
}

pub(crate) fn clear_last_error() {
    if let Ok(mut guard) = LAST_ERROR.lock() {
        guard.clear();
    }
}

pub(crate) fn last_error_string() -> String {
    LAST_ERROR.lock().map(|g| g.clone()).unwrap_or_default()
}

// ---- Non-Windows stub ------------------------------------------------------

#[cfg(not(windows))]
pub(crate) mod imp {
    use super::*;

    #[derive(Default)]
    pub(crate) struct ComTable;

    impl ComTable {
        pub(crate) fn new() -> Self { Self }
        pub(crate) fn create(&mut self, _progid: &str) -> i32 {
            set_last_error("COM is not supported on this target");
            0
        }
        pub(crate) fn release(&mut self, _handle: i32) {}
        pub(crate) fn has_member(&self, _handle: i32, _name: &str) -> bool {
            set_last_error("COM is not supported on this target");
            false
        }
        pub(crate) fn call_i(&mut self, _h: i32, _name: &str, _args: &[Arg]) -> i32 {
            set_last_error("COM is not supported on this target");
            i32::MIN
        }
        pub(crate) fn call_s(&mut self, _h: i32, _name: &str, _args: &[Arg]) -> Option<String> {
            set_last_error("COM is not supported on this target");
            None
        }
        pub(crate) fn get_i(&mut self, _h: i32, _name: &str) -> i32 {
            set_last_error("COM is not supported on this target");
            i32::MIN
        }
        pub(crate) fn get_s(&mut self, _h: i32, _name: &str) -> Option<String> {
            set_last_error("COM is not supported on this target");
            None
        }
        pub(crate) fn set_i(&mut self, _h: i32, _name: &str, _v: i32) -> bool {
            set_last_error("COM is not supported on this target");
            false
        }
        pub(crate) fn set_s(&mut self, _h: i32, _name: &str, _v: &str) -> bool {
            set_last_error("COM is not supported on this target");
            false
        }
    }

    pub(crate) enum Arg<'a> {
        I32(i32),
        Str(&'a str),
    }
}

// ---- Windows implementation ------------------------------------------------

#[cfg(windows)]
pub(crate) mod imp {
    use super::*;
    use windows::core::{BSTR, GUID, HSTRING, PCWSTR, VARIANT};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, CLSIDFromProgID, CoCreateInstance,
        CoInitializeEx, CoUninitialize, IDispatch, COINIT_APARTMENTTHREADED, DISPATCH_METHOD,
        DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS, EXCEPINFO,
    };
    use windows::Win32::System::Ole::DISPID_PROPERTYPUT;

    pub(crate) enum Arg<'a> {
        I32(i32),
        Str(&'a str),
    }

    struct Slot {
        disp: Option<IDispatch>,
    }

    pub(crate) struct ComTable {
        slots: Vec<Option<Slot>>,
        initialized: bool,
    }

    impl Default for ComTable {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ComTable {
        pub(crate) fn new() -> Self {
            Self { slots: Vec::new(), initialized: false }
        }

        fn ensure_init(&mut self) -> Result<(), String> {
            if self.initialized {
                return Ok(());
            }
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if hr.is_err() {
                return Err(format!("CoInitializeEx failed: 0x{:08x}", hr.0));
            }
            self.initialized = true;
            Ok(())
        }

        pub(crate) fn create(&mut self, progid: &str) -> i32 {
            clear_last_error();
            if let Err(e) = self.ensure_init() {
                set_last_error(e);
                return 0;
            }
            let hprog = HSTRING::from(progid);
            let clsid = match unsafe { CLSIDFromProgID(&hprog) } {
                Ok(c) => c,
                Err(e) => {
                    set_last_error(format!("CLSIDFromProgID({progid}) failed: {e}"));
                    return 0;
                }
            };
            let disp: Result<IDispatch, _> = unsafe {
                CoCreateInstance(&clsid, None, CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER)
            };
            match disp {
                Ok(d) => {
                    self.slots.push(Some(Slot { disp: Some(d) }));
                    self.slots.len() as i32
                }
                Err(e) => {
                    set_last_error(format!("CoCreateInstance({progid}) failed: {e}"));
                    0
                }
            }
        }

        pub(crate) fn release(&mut self, handle: i32) {
            if handle <= 0 {
                return;
            }
            if let Some(slot) = self.slots.get_mut((handle - 1) as usize) {
                *slot = None;
            }
        }

        fn with_disp<R>(
            &self,
            handle: i32,
            f: impl FnOnce(&IDispatch) -> Result<R, String>,
        ) -> Result<R, String> {
            if handle <= 0 {
                return Err("invalid COM handle".into());
            }
            let slot = self
                .slots
                .get((handle - 1) as usize)
                .and_then(|s| s.as_ref())
                .ok_or_else(|| "invalid COM handle".to_string())?;
            let disp = slot
                .disp
                .as_ref()
                .ok_or_else(|| "COM object already released".to_string())?;
            f(disp)
        }

        fn get_dispid(disp: &IDispatch, name: &str) -> Result<i32, String> {
            let wide: Vec<u16> =
                name.encode_utf16().chain(std::iter::once(0)).collect();
            let name_ptr = PCWSTR::from_raw(wide.as_ptr());
            let names = [name_ptr];
            let mut dispid: i32 = 0;
            let iid = GUID::zeroed();
            unsafe {
                disp.GetIDsOfNames(&iid, names.as_ptr(), 1, 0, &mut dispid)
                    .map_err(|e| format!("GetIDsOfNames({name}) failed: {e}"))?;
            }
            Ok(dispid)
        }

        pub(crate) fn has_member(&self, handle: i32, name: &str) -> bool {
            clear_last_error();
            match self.with_disp(handle, |d| Self::get_dispid(d, name).map(|_| ())) {
                Ok(()) => true,
                Err(e) => {
                    set_last_error(e);
                    false
                }
            }
        }

        fn invoke(
            disp: &IDispatch,
            name: &str,
            kind: u8,
            args: &[Arg],
        ) -> Result<VariantOut, String> {
            let dispid = Self::get_dispid(disp, name)?;

            // Build VARIANT args in reverse order (COM convention) using
            // the high-level windows::core::VARIANT From impls.
            let mut variants: Vec<VARIANT> = Vec::with_capacity(args.len());
            for a in args.iter().rev() {
                match a {
                    Arg::I32(i) => variants.push(VARIANT::from(*i)),
                    Arg::Str(s) => variants.push(VARIANT::from(BSTR::from(*s))),
                }
            }

            let mut put_dispid = DISPID_PROPERTYPUT;
            let mut params = DISPPARAMS::default();
            params.cArgs = variants.len() as u32;
            if !variants.is_empty() {
                params.rgvarg = variants.as_mut_ptr();
            }
            // For method/getter calls pass BOTH flags — COM's late-binding
            // convention (also what VBScript does) so members that are
            // really property getters (e.g. Scripting.Dictionary.Item)
            // resolve correctly when the script uses com_call_*.
            let flags = match kind {
                1 => DISPATCH_METHOD | DISPATCH_PROPERTYGET,
                2 => DISPATCH_PROPERTYGET,
                3 => {
                    params.cNamedArgs = 1;
                    params.rgdispidNamedArgs = &mut put_dispid;
                    DISPATCH_PROPERTYPUT
                }
                _ => DISPATCH_METHOD,
            };

            let mut result = VARIANT::new();
            let mut excep = EXCEPINFO::default();
            let mut arg_err: u32 = 0;
            let iid = GUID::zeroed();
            unsafe {
                disp.Invoke(
                    dispid,
                    &iid,
                    0,
                    flags,
                    &params,
                    Some(&mut result),
                    Some(&mut excep),
                    Some(&mut arg_err),
                )
                .map_err(|e| format!("Invoke({name}) failed: {e}"))?;
            }
            drop(variants);

            // The public `TryFrom<&VARIANT>` impls in `windows-core` all
            // route through `propsys.dll` (VariantToInt32 / PropVariantToBSTR),
            // which wine does not implement. Instead, pun the VARIANT to its
            // ABI-stable C layout and read the field that matches the vt tag.
            // VARIANT layout (Win32, 64-bit): { vt: u16, 3 * u16 reserved,
            // union[16 bytes] } — total 24 bytes.
            let out = unsafe { unpack_variant(&result) };
            Ok(out)
        }

        pub(crate) fn call_i(&mut self, handle: i32, name: &str, args: &[Arg]) -> i32 {
            clear_last_error();
            match self.with_disp(handle, |d| Self::invoke(d, name, 1, args)) {
                Ok(VariantOut::I32(i)) => i,
                Ok(VariantOut::Unit) => 0,
                Ok(VariantOut::Str(_)) => {
                    set_last_error("expected i32 return, got string");
                    i32::MIN
                }
                Err(e) => {
                    set_last_error(e);
                    i32::MIN
                }
            }
        }

        pub(crate) fn call_s(&mut self, handle: i32, name: &str, args: &[Arg]) -> Option<String> {
            clear_last_error();
            match self.with_disp(handle, |d| Self::invoke(d, name, 1, args)) {
                Ok(VariantOut::Str(s)) => Some(s),
                Ok(VariantOut::I32(i)) => Some(i.to_string()),
                Ok(VariantOut::Unit) => Some(String::new()),
                Err(e) => {
                    set_last_error(e);
                    None
                }
            }
        }

        pub(crate) fn get_i(&mut self, handle: i32, name: &str) -> i32 {
            clear_last_error();
            match self.with_disp(handle, |d| Self::invoke(d, name, 2, &[])) {
                Ok(VariantOut::I32(i)) => i,
                Ok(VariantOut::Unit) => 0,
                Ok(VariantOut::Str(_)) => {
                    set_last_error("expected i32 property, got string");
                    i32::MIN
                }
                Err(e) => {
                    set_last_error(e);
                    i32::MIN
                }
            }
        }

        pub(crate) fn get_s(&mut self, handle: i32, name: &str) -> Option<String> {
            clear_last_error();
            match self.with_disp(handle, |d| Self::invoke(d, name, 2, &[])) {
                Ok(VariantOut::Str(s)) => Some(s),
                Ok(VariantOut::I32(i)) => Some(i.to_string()),
                Ok(VariantOut::Unit) => Some(String::new()),
                Err(e) => {
                    set_last_error(e);
                    None
                }
            }
        }

        pub(crate) fn set_i(&mut self, handle: i32, name: &str, v: i32) -> bool {
            clear_last_error();
            match self.with_disp(handle, |d| Self::invoke(d, name, 3, &[Arg::I32(v)])) {
                Ok(_) => true,
                Err(e) => {
                    set_last_error(e);
                    false
                }
            }
        }

        pub(crate) fn set_s(&mut self, handle: i32, name: &str, v: &str) -> bool {
            clear_last_error();
            match self.with_disp(handle, |d| Self::invoke(d, name, 3, &[Arg::Str(v)])) {
                Ok(_) => true,
                Err(e) => {
                    set_last_error(e);
                    false
                }
            }
        }
    }

    impl Drop for ComTable {
        fn drop(&mut self) {
            self.slots.clear();
            if self.initialized {
                unsafe { CoUninitialize() };
                self.initialized = false;
            }
        }
    }

    enum VariantOut {
        Unit,
        I32(i32),
        Str(String),
    }

    // Well-known Win32 VARIANT tag values.
    const VT_EMPTY: u16 = 0;
    const VT_NULL: u16 = 1;
    const VT_I2: u16 = 2;
    const VT_I4: u16 = 3;
    const VT_R4: u16 = 4;
    const VT_R8: u16 = 5;
    const VT_BSTR: u16 = 8;
    const VT_BOOL: u16 = 11;
    const VT_I1: u16 = 16;
    const VT_UI1: u16 = 17;
    const VT_UI2: u16 = 18;
    const VT_UI4: u16 = 19;
    const VT_I8: u16 = 20;
    const VT_UI8: u16 = 21;
    const VT_INT: u16 = 22;
    const VT_UINT: u16 = 23;

    #[repr(C)]
    struct RawVariant {
        vt: u16,
        _reserved: [u16; 3],
        payload: [u8; 16],
    }

    /// Read a field of type `T` out of the VARIANT payload union.
    unsafe fn read_payload<T: Copy>(rv: &RawVariant) -> T {
        unsafe { core::ptr::read_unaligned(rv.payload.as_ptr() as *const T) }
    }

    /// Copy a BSTR pointer (a UTF-16 string with a 4-byte length prefix)
    /// into an owned `String`. Uses the length prefix so embedded NULs
    /// don't truncate.
    unsafe fn bstr_to_string(ptr: *const u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        unsafe {
            // BSTR length (in bytes) sits immediately before the data.
            let len_bytes =
                core::ptr::read_unaligned((ptr as *const u8).sub(4) as *const u32);
            let len_u16 = (len_bytes / 2) as usize;
            let slice = core::slice::from_raw_parts(ptr, len_u16);
            String::from_utf16_lossy(slice)
        }
    }

    unsafe fn unpack_variant(v: &VARIANT) -> VariantOut {
        // `windows::core::VARIANT` is `#[repr(transparent)]` over the
        // Win32 VARIANT — it's safe to pun to an identically-shaped
        // `#[repr(C)]` clone of the documented layout.
        unsafe {
            let rv: &RawVariant = &*(v as *const VARIANT as *const RawVariant);
            match rv.vt {
                VT_EMPTY | VT_NULL => VariantOut::Unit,
                VT_I2 => VariantOut::I32(read_payload::<i16>(rv) as i32),
                VT_I4 | VT_INT => VariantOut::I32(read_payload::<i32>(rv)),
                VT_I1 => VariantOut::I32(read_payload::<i8>(rv) as i32),
                VT_UI1 => VariantOut::I32(read_payload::<u8>(rv) as i32),
                VT_UI2 => VariantOut::I32(read_payload::<u16>(rv) as i32),
                VT_UI4 | VT_UINT => VariantOut::I32(read_payload::<u32>(rv) as i32),
                VT_I8 => VariantOut::I32(read_payload::<i64>(rv) as i32),
                VT_UI8 => VariantOut::I32(read_payload::<u64>(rv) as i32),
                VT_R4 => VariantOut::I32(read_payload::<f32>(rv) as i32),
                VT_R8 => VariantOut::I32(read_payload::<f64>(rv) as i32),
                VT_BOOL => {
                    // VARIANT_BOOL: 0 = false, -1 (0xFFFF) = true.
                    let b = read_payload::<i16>(rv);
                    VariantOut::I32(if b != 0 { 1 } else { 0 })
                }
                VT_BSTR => {
                    let p = read_payload::<*mut u16>(rv);
                    VariantOut::Str(bstr_to_string(p))
                }
                _ => VariantOut::Unit,
            }
        }
    }
}

pub(crate) use imp::{Arg, ComTable};
