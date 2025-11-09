// src/single_instance.rs
//! Single instance enforcement for SkylineDB.
//! Ensures only one instance of the application can run at a time.
//! On Windows, uses a named mutex for detection and named pipes for IPC.

#[cfg(target_os = "windows")]
use std::ptr::null_mut;
#[cfg(target_os = "windows")]
use winapi::um::synchapi::CreateMutexW;
#[cfg(target_os = "windows")]
use winapi::um::synchapi::ReleaseMutex;
#[cfg(target_os = "windows")]
use winapi::um::handleapi::CloseHandle;
#[cfg(target_os = "windows")]
use winapi::um::errhandlingapi::GetLastError;
#[cfg(target_os = "windows")]
use winapi::um::winnt::HANDLE;
#[cfg(target_os = "windows")]
use winapi::shared::winerror::ERROR_ALREADY_EXISTS;

const MUTEX_NAME: &str = "SkylineDB_SingleInstance_Mutex";
const PIPE_NAME: &str = "\\\\.\\pipe\\SkylineDB_SingleInstance";

#[cfg(target_os = "windows")]
pub struct SingleInstanceGuard {
    mutex_handle: HANDLE,
}

#[cfg(target_os = "windows")]
impl SingleInstanceGuard {
    /// Attempts to acquire the single instance lock.
    /// Returns Some(guard) if this is the first instance, None if another instance is already running.
    pub fn try_acquire() -> Option<Self> {
        unsafe {
            let mutex_name: Vec<u16> = MUTEX_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let mutex_handle = CreateMutexW(null_mut(), 0, mutex_name.as_ptr());
            
            if mutex_handle.is_null() {
                eprintln!("Failed to create mutex");
                return None;
            }

            let error = GetLastError();
            if error == ERROR_ALREADY_EXISTS {
                // Another instance is running
                CloseHandle(mutex_handle);
                return None;
            }

            Some(SingleInstanceGuard { mutex_handle })
        }
    }

    /// Signals the existing instance to bring itself to the foreground.
    pub fn signal_existing_instance() {
        use winapi::um::fileapi::CreateFileW;
        use winapi::um::winbase::FILE_FLAG_OVERLAPPED;
        use winapi::um::fileapi::OPEN_EXISTING;
        use winapi::um::winnt::{GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE};

        unsafe {
            let pipe_name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let pipe_handle = CreateFileW(
                pipe_name.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                null_mut(),
            );

            if pipe_handle == winapi::um::handleapi::INVALID_HANDLE_VALUE {
                eprintln!("Could not connect to existing instance pipe");
                return;
            }

            // Send a simple message to signal the existing instance
            let message = b"SHOW";
            let mut bytes_written: u32 = 0;
            winapi::um::fileapi::WriteFile(
                pipe_handle,
                message.as_ptr() as *const _,
                message.len() as u32,
                &mut bytes_written,
                null_mut(),
            );

            CloseHandle(pipe_handle);
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.mutex_handle.is_null() {
                ReleaseMutex(self.mutex_handle);
                CloseHandle(self.mutex_handle);
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub fn start_ipc_listener() -> std::sync::mpsc::Receiver<()> {
    use std::sync::mpsc;
    use std::thread;
    use winapi::um::namedpipeapi::CreateNamedPipeW;
    use winapi::um::namedpipeapi::ConnectNamedPipe;
    use winapi::um::winbase::{PIPE_ACCESS_INBOUND, PIPE_TYPE_BYTE, PIPE_READMODE_BYTE, PIPE_WAIT};

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        loop {
            unsafe {
                let pipe_name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
                let pipe_handle = CreateNamedPipeW(
                    pipe_name.as_ptr(),
                    PIPE_ACCESS_INBOUND,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                    1, // max instances
                    512, // out buffer size
                    512, // in buffer size
                    0, // default timeout
                    null_mut(),
                );

                if pipe_handle == winapi::um::handleapi::INVALID_HANDLE_VALUE {
                    eprintln!("Failed to create named pipe");
                    break;
                }

                // Wait for a client to connect
                let connected = ConnectNamedPipe(pipe_handle, null_mut());
                if connected != 0 || GetLastError() == winapi::shared::winerror::ERROR_PIPE_CONNECTED {
                    // Signal the main thread to show the window
                    let _ = tx.send(());
                }

                CloseHandle(pipe_handle);
            }
        }
    });

    rx
}

#[cfg(not(target_os = "windows"))]
pub struct SingleInstanceGuard;

#[cfg(not(target_os = "windows"))]
impl SingleInstanceGuard {
    pub fn try_acquire() -> Option<Self> {
        // On non-Windows platforms, always allow the instance for now
        Some(SingleInstanceGuard)
    }

    pub fn signal_existing_instance() {
        // No-op on non-Windows
    }
}

#[cfg(not(target_os = "windows"))]
pub fn start_ipc_listener() -> std::sync::mpsc::Receiver<()> {
    let (_, rx) = std::sync::mpsc::channel();
    rx
}
