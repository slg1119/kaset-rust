use std::ffi::{OsStr, OsString, c_void};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr::{null_mut, slice_from_raw_parts};
use std::time::Duration;

use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows_sys::Win32::Foundation::{ERROR_PIPE_BUSY, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, OWNER_SECURITY_INFORMATION, PSID, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;

use super::ClientError;

pub(super) fn endpoint() -> io::Result<OsString> {
    let mut name = [0; 257]; // UNLEN + the terminating NUL
    let mut len = name.len() as u32;
    // SAFETY: name holds len writable UTF-16 code units.
    if unsafe { GetUserNameW(name.as_mut_ptr(), &mut len) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut path = OsString::from(r"\\.\pipe\kaset-");
    path.push(OsString::from_wide(&name[..len as usize - 1]));
    Ok(path)
}

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: successful Win32 allocation calls below return LocalFree memory.
        unsafe { LocalFree(self.0) };
    }
}

/// Returns an aligned buffer owning the token user record and its SID.
fn token_user() -> io::Result<Vec<usize>> {
    // SAFETY: OpenProcessToken initializes raw on success; OwnedHandle closes it.
    let token = unsafe {
        let mut raw = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) == 0 {
            return Err(io::Error::last_os_error());
        }
        OwnedHandle::from_raw_handle(raw)
    };
    let mut len = 0;
    // SAFETY: a null buffer with zero length queries the required size.
    unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut len) };
    if len < size_of::<TOKEN_USER>() as u32 {
        return Err(io::Error::last_os_error());
    }
    let mut user = vec![0; (len as usize).div_ceil(size_of::<usize>())];
    // SAFETY: user is aligned and has room for len bytes.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            user.as_mut_ptr().cast(),
            len,
            &mut len,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(user)
}

fn user_sid_string() -> io::Result<String> {
    let user = token_user()?;
    let mut sid_string = null_mut();
    // SAFETY: token_user returned a populated TOKEN_USER; its SID lives in user.
    let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    if unsafe { ConvertSidToStringSidW(sid, &mut sid_string) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _allocation = LocalAllocation(sid_string.cast());
    // SAFETY: ConvertSidToStringSidW returns a NUL-terminated UTF-16 string.
    let units = unsafe {
        let len = (0..).find(|&index| *sid_string.add(index) == 0).unwrap();
        &*slice_from_raw_parts(sid_string, len)
    };
    String::from_utf16(units).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn create_pipe(path: &OsStr, sddl: &[u16], first: bool) -> io::Result<NamedPipeServer> {
    let mut descriptor = null_mut();
    // SAFETY: sddl is NUL-terminated, and descriptor receives an owned allocation.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let _allocation = LocalAllocation(descriptor);
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    // SAFETY: attributes and its initialized descriptor live through CreateNamedPipe.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                path,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
            )
    }
}

pub(super) struct Server {
    listener: NamedPipeServer,
    path: OsString,
    sddl: Vec<u16>,
}

impl Server {
    pub(super) fn bind(path: &OsStr) -> io::Result<Self> {
        let sid = user_sid_string()?;
        // Set the owner explicitly (the default owner can be Administrators).
        // A protected DACL grants access only to the process token's user.
        let sddl = OsStr::new(&format!("O:{sid}D:P(A;;GA;;;{sid})"))
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>();
        Ok(Self {
            listener: create_pipe(path, &sddl, true)?,
            path: path.to_owned(),
            sddl,
        })
    }

    pub(super) async fn accept(&mut self) -> io::Result<NamedPipeServer> {
        self.listener.connect().await?;
        // Keep one instance listening while the accepted connection is served.
        let next = create_pipe(&self.path, &self.sddl, false)?;
        Ok(std::mem::replace(&mut self.listener, next))
    }
}

fn check_owner(client: &NamedPipeClient) -> io::Result<()> {
    let user = token_user()?;
    let mut descriptor = null_mut();
    let mut owner: PSID = null_mut();
    // SAFETY: client holds the handle; the returned owner SID lives in descriptor.
    let result = unsafe {
        GetSecurityInfo(
            client.as_raw_handle(),
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let _allocation = LocalAllocation(descriptor);
    // SAFETY: both buffers own valid SIDs for the duration of EqualSid.
    if owner.is_null()
        || unsafe { EqualSid(owner, (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid) } == 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unexpected Kaset pipe owner",
        ));
    }
    Ok(())
}

pub(super) async fn connect(path: &OsStr) -> Result<NamedPipeClient, ClientError> {
    loop {
        // Tokio defaults to SECURITY_IDENTIFICATION, so a server cannot impersonate the CLI.
        match ClientOptions::new().open(path) {
            Ok(client) => {
                check_owner(&client)?;
                return Ok(client);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(ClientError::NotRunning);
            }
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{exchange, serve_from};
    use crate::{Command, Response};

    #[tokio::test]
    async fn windows_pipe_round_trips_and_keeps_one_listener() {
        let mut path = endpoint().unwrap();
        path.push(format!("-test-{}", std::process::id()));
        let path = path.as_os_str();
        let listener = Server::bind(path).unwrap();
        assert!(Server::bind(path).is_err());
        let server = tokio::spawn(serve_from(listener, |command| async move {
            assert_eq!(command, Command::Play {});
            Response::Ok
        }));
        for _ in 0..2 {
            let stream = connect(path).await.unwrap();
            assert_eq!(
                exchange(stream, &Command::Play {}).await.unwrap(),
                Response::Ok
            );
        }
        server.abort();
        let _ = server.await;
        assert!(matches!(connect(path).await, Err(ClientError::NotRunning)));
    }
}
