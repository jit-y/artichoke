use crate::extn::prelude::*;
use crate::extn::stdlib::securerandom;

#[inline]
pub fn alphanumeric(interp: &mut Artichoke, len: Option<Value>) -> Result<Value, Exception> {
    let alpha = if let Some(len) = len {
        let len = len.implicitly_convert_to_int(interp)?;
        securerandom::alphanumeric(Some(len))?
    } else {
        securerandom::alphanumeric(None)?
    };
    Ok(interp.convert_mut(alpha))
}

#[inline]
pub fn base64(interp: &mut Artichoke, len: Option<Value>) -> Result<Value, Exception> {
    let base64 = if let Some(len) = len {
        let len = len.implicitly_convert_to_int(interp)?;
        securerandom::base64(Some(len))?
    } else {
        securerandom::base64(None)?
    };
    Ok(interp.convert_mut(base64))
}

#[inline]
pub fn hex(interp: &mut Artichoke, len: Option<Value>) -> Result<Value, Exception> {
    let hex = if let Some(len) = len {
        let len = len.implicitly_convert_to_int(interp)?;
        securerandom::hex(Some(len))?
    } else {
        securerandom::hex(None)?
    };
    Ok(interp.convert_mut(hex))
}

#[inline]
pub fn random_bytes(interp: &mut Artichoke, len: Option<Value>) -> Result<Value, Exception> {
    let bytes = if let Some(len) = len {
        let len = len.implicitly_convert_to_int(interp)?;
        securerandom::random_bytes(Some(len))?
    } else {
        securerandom::random_bytes(None)?
    };
    Ok(interp.convert_mut(bytes))
}

#[inline]
pub fn random_number(interp: &mut Artichoke, max: Option<Value>) -> Result<Value, Exception> {
    let max = interp.try_convert_mut(max)?;
    let num = securerandom::random_number(max)?;
    Ok(interp.convert_mut(num))
}

#[inline]
pub fn uuid(interp: &mut Artichoke) -> Result<Value, Exception> {
    let uuid = securerandom::uuid();
    Ok(interp.convert_mut(uuid))
}
