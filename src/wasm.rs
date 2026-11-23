//! The checker for a web page. Built for `wasm32-unknown-unknown` the library exports three
//! plain functions and needs no generated glue:
//!
//! 1. `zetema_alloc(len)` gives the page `len` bytes to write a certificate into.
//! 2. `zetema_check(ptr, len)` reads the certificate, frees those bytes, and returns the length
//!    of its answer.
//! 3. `zetema_answer()` points at that answer: one line of JSON, valid until the next check.

use crate::bound::schoenheim;
use crate::check::check;
use crate::design::Covering;
use std::sync::Mutex;

static ANSWER: Mutex<String> = Mutex::new(String::new());

#[no_mangle]
pub extern "C" fn zetema_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()).cast()
}

/// # Safety
/// `ptr` must come from `zetema_alloc(len)` with this same `len`, and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn zetema_check(ptr: *mut u8, len: usize) -> usize {
    // SAFETY: by the contract above this is the boxed slice `zetema_alloc` leaked.
    let bytes = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) };
    let answer = match std::str::from_utf8(&bytes) {
        Ok(text) => judge(text),
        Err(_) => refuse("the certificate is not UTF-8 text"),
    };
    let mut slot = ANSWER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = answer;
    slot.len()
}

#[no_mangle]
pub extern "C" fn zetema_answer() -> *const u8 {
    ANSWER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ptr()
}

fn refuse(reason: &str) -> String {
    let escaped: String = reason.chars().flat_map(|c| c.escape_default()).collect();
    format!("{{\"ok\":false,\"reason\":\"{escaped}\"}}")
}

fn judge(text: &str) -> String {
    let design = match Covering::parse(text) {
        Ok(design) => design,
        Err(error) => return refuse(&error.to_string()),
    };
    let report = match check(&design) {
        Ok(report) => report,
        Err(fault) => return refuse(&fault.to_string()),
    };
    let witness: Vec<String> = report
        .witness
        .iter()
        .flatten()
        .map(|&point| (usize::from(point) + 1).to_string())
        .collect();
    format!(
        "{{\"ok\":{},\"v\":{},\"k\":{},\"t\":{},\"blocks\":{},\"sets\":{},\"uncovered\":{},\"witness\":[{}],\"bound\":{},\"sha256\":\"{}\"}}",
        report.is_covering(),
        design.v,
        design.k,
        design.t,
        report.blocks,
        report.sets,
        report.uncovered,
        witness.join(","),
        schoenheim(design.v as u64, design.k as u64, design.t as u64),
        design.digest()
    )
}
