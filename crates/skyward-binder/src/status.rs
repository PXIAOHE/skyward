use crate::parcel::Reader;
use anyhow::{bail, Context, Result};

const NONE: i32 = 0;
const SECURITY: i32 = -1;
const BAD_PARCELABLE: i32 = -2;
const ILLEGAL_ARGUMENT: i32 = -3;
const NULL_POINTER: i32 = -4;
const ILLEGAL_STATE: i32 = -5;
const UNSUPPORTED_OPERATION: i32 = -7;
const SERVICE_SPECIFIC: i32 = -8;
const TRANSACTION_FAILED: i32 = -129;

fn name(exception: i32) -> &'static str {
    match exception {
        SECURITY => "a refusal on security grounds",
        BAD_PARCELABLE => "a value it could not read",
        ILLEGAL_ARGUMENT => "an argument it would not take",
        NULL_POINTER => "an argument that was missing",
        ILLEGAL_STATE => "a state it was not ready for",
        UNSUPPORTED_OPERATION => "something it does not do",
        TRANSACTION_FAILED => "a failure in the driver",
        _ => "a failure",
    }
}

/// Read the mark every reply opens with, leaving the reader on whatever the
/// method itself answered. A service that refused the call says so here, and
/// its own error number comes with it.
pub fn ok(r: &mut Reader) -> Result<()> {
    match r.i32().context("read how the call ended")? {
        NONE => Ok(()),
        SERVICE_SPECIFIC => {
            let said = r.string().unwrap_or_default();
            let code = r.i32().unwrap_or(0);
            bail!("the service refused the call with its error {code}{}", aside(&said));
        }
        other => {
            let said = r.string().unwrap_or_default();
            bail!("the call ended in {}{}", name(other), aside(&said));
        }
    }
}

fn aside(said: &str) -> String {
    if said.is_empty() {
        String::new()
    } else {
        format!(" ({said})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parcel::Parcel;

    #[test]
    fn a_reply_that_went_through_leaves_the_rest_to_be_read() {
        let mut p = Parcel::default();
        p.i32(NONE);
        p.i32(1234);
        let bytes = p.bytes().to_vec();
        let mut r = Reader::new(&bytes);
        ok(&mut r).unwrap();
        assert_eq!(r.i32().unwrap(), 1234);
    }

    #[test]
    fn a_services_own_error_number_is_reported() {
        let mut p = Parcel::default();
        p.i32(SERVICE_SPECIFIC);
        p.string("no such key");
        p.i32(-38);
        let bytes = p.bytes().to_vec();
        let said = format!("{:#}", ok(&mut Reader::new(&bytes)).unwrap_err());
        assert!(said.contains("-38") && said.contains("no such key"), "{said}");
    }

    #[test]
    fn a_refusal_by_the_binder_layer_is_named() {
        let mut p = Parcel::default();
        p.i32(SECURITY);
        p.string("");
        let bytes = p.bytes().to_vec();
        let said = format!("{:#}", ok(&mut Reader::new(&bytes)).unwrap_err());
        assert!(said.contains("security"), "{said}");
    }
}
