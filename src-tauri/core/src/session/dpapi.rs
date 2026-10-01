//! Windows DPAPI (CryptProtectData / CryptUnprotectData, CurrentUser スコープ) による暗号化。
//!
//! Windows 以外では暗号化せずそのまま返す (ファイル自体は 0o600 で保存される。開発用)。

/// 暗号化。失敗時はユーザー向けメッセージを返す。
pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    imp::protect(plain)
}

/// 復号。失敗時はユーザー向けメッセージを返す。
pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    imp::unprotect(blob)
}

#[cfg(windows)]
mod imp {
    use std::ptr;

    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    /// 復号時に説明文字列と一致していなくても復号はできるが、識別用に付けておく。
    const DESCRIPTION: &[u16] = &[
        b'V' as u16,
        b'R' as u16,
        b'C' as u16,
        b'I' as u16,
        b'n' as u16,
        b'v' as u16,
        b'i' as u16,
        b't' as u16,
        b'e' as u16,
        b'T' as u16,
        b'o' as u16,
        b'o' as u16,
        b'l' as u16,
        0,
    ];

    fn blob_of(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    /// 出力 BLOB を Vec にコピーして LocalFree する。
    unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let v = if out.pbData.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec()
        };
        if !out.pbData.is_null() {
            LocalFree(out.pbData as *mut core::ffi::c_void);
        }
        v
    }

    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob_of(plain);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        // SAFETY: input はこの関数のスコープ内で有効。out は API が確保し、take で解放する。
        let ok = unsafe {
            CryptProtectData(
                &input,
                DESCRIPTION.as_ptr(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            let code = unsafe { GetLastError() };
            return Err(format!("DPAPI による暗号化に失敗しました (エラー {code})"));
        }
        Ok(unsafe { take(out) })
    }

    pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob_of(blob);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        // SAFETY: protect と同様。説明文字列は不要なので null。
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            let code = unsafe { GetLastError() };
            return Err(format!(
                "保存されたセッションを復号できません (エラー {code})。別のユーザー/PC で保存されたファイルの可能性があります"
            ));
        }
        Ok(unsafe { take(out) })
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
        Ok(plain.to_vec())
    }

    pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
        Ok(blob.to_vec())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        let data = b"[{\"name\":\"auth\"}]";
        let enc = super::protect(data).unwrap();
        assert_eq!(super::unprotect(&enc).unwrap(), data);
    }
}
