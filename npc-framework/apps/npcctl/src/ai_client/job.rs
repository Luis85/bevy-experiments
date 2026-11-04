//! Windows closes the owned provider process tree even if its supervising client crashes.
//! Other platforms place owned children in a process group and terminate that group on stop.
#[cfg(windows)]
mod windows {
    use std::{ffi::c_void, os::windows::io::AsRawHandle, process::Child};
    type Handle = *mut c_void;
    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        process_time: i64,
        job_time: i64,
        flags: u32,
        minimum_working_set: usize,
        maximum_working_set: usize,
        active_processes: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operations: u64,
        write_operations: u64,
        other_operations: u64,
        read_bytes: u64,
        write_bytes: u64,
        other_bytes: u64,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: IoCounters,
        process_memory: usize,
        job_memory: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: u32,
            information: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    pub struct Job(Handle);
    impl Job {
        pub fn attach(child: &Child) -> Result<Self, String> {
            // SAFETY: null security/name arguments request a private non-inheritable job handle.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err("cannot create owned provider process job".into());
            }
            let job = Self(handle);
            let limits = ExtendedLimits {
                basic: BasicLimits {
                    flags: 0x2000,
                    ..Default::default()
                },
                ..Default::default()
            };
            // SAFETY: the ABI layouts above match JOBOBJECT_EXTENDED_LIMIT_INFORMATION;
            // the handle is live and both structures remain valid for the synchronous call.
            let configured = unsafe {
                SetInformationJobObject(
                    handle,
                    9,
                    std::ptr::from_ref(&limits).cast(),
                    std::mem::size_of::<ExtendedLimits>() as u32,
                )
            };
            // SAFETY: Child owns a live process handle; assigning it never transfers that ownership.
            let assigned = configured != 0
                && unsafe { AssignProcessToJobObject(handle, child.as_raw_handle()) } != 0;
            if !assigned {
                return Err(
                    "cannot isolate owned provider processes; AI turn was not started".into(),
                );
            }
            Ok(job)
        }
    }
    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: this wrapper uniquely owns a valid job handle. Closing it kills only its members.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}
#[cfg(windows)]
pub use windows::Job;
#[cfg(not(windows))]
pub struct Job;
#[cfg(not(windows))]
impl Job {
    pub fn attach(_child: &std::process::Child) -> Result<Self, String> {
        Ok(Self)
    }
}
