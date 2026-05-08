#[cfg(any(target_os = "macos", target_os = "ios"))]
#[allow(deprecated)]
pub fn get_memory_usage() -> Option<u64> {
    use libc::{KERN_SUCCESS, mach_msg_type_number_t, mach_task_self, time_value_t};
    use std::mem;

    // Define mach_task_basic_info for 64-bit sizes
    #[repr(C)]
    struct mach_task_basic_info {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: time_value_t,
        system_time: time_value_t,
        policy: i32,
        suspend_count: i32,
    }

    // MACH_TASK_BASIC_INFO is 20
    const MACH_TASK_BASIC_INFO: u32 = 20;

    let mut info: mach_task_basic_info = unsafe { mem::zeroed() };
    let mut count: mach_msg_type_number_t =
        (mem::size_of::<mach_task_basic_info>() / mem::size_of::<i32>()) as mach_msg_type_number_t;

    let res = unsafe {
        libc::task_info(
            mach_task_self(),
            MACH_TASK_BASIC_INFO,
            &mut info as *mut _ as *mut i32,
            &mut count,
        )
    };

    if res == KERN_SUCCESS {
        return Some(info.resident_size);
    }
    None
}

#[cfg(target_os = "linux")]
pub fn get_memory_usage() -> Option<u64> {
    use std::fs;

    // Read RSS from /proc/self/statm (2nd field)
    let content = fs::read_to_string("/proc/self/statm").ok()?;
    let mut parts = content.split_whitespace();
    // 1st: total program size
    // 2nd: resident set size
    let _total = parts.next()?;
    let rss_pages = parts.next()?.parse::<u64>().ok()?;

    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size > 0 {
        return Some(rss_pages * page_size as u64);
    }
    None
}

#[cfg(target_os = "android")]
pub mod android_alloc {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub struct TrackingAllocator {
        pub allocated: AtomicUsize,
        pub freed: AtomicUsize,
    }

    impl TrackingAllocator {
        pub const fn new() -> Self {
            TrackingAllocator {
                allocated: AtomicUsize::new(0),
                freed: AtomicUsize::new(0),
            }
        }

        pub fn current_usage(&self) -> usize {
            let allocated = self.allocated.load(Ordering::Relaxed);
            let freed = self.freed.load(Ordering::Relaxed);
            if allocated > freed {
                allocated - freed
            } else {
                0
            }
        }
    }

    unsafe impl GlobalAlloc for TrackingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = System.alloc(layout);
            if !ptr.is_null() {
                self.allocated.fetch_add(layout.size(), Ordering::Relaxed);
            }
            ptr
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout);
            self.freed.fetch_add(layout.size(), Ordering::Relaxed);
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let ptr = System.alloc_zeroed(layout);
            if !ptr.is_null() {
                self.allocated.fetch_add(layout.size(), Ordering::Relaxed);
            }
            ptr
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let new_ptr = System.realloc(ptr, layout, new_size);
            if !new_ptr.is_null() {
                self.freed.fetch_add(layout.size(), Ordering::Relaxed);
                self.allocated.fetch_add(new_size, Ordering::Relaxed);
            }
            new_ptr
        }
    }

    #[global_allocator]
    pub static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();
}

#[cfg(target_os = "android")]
pub fn get_memory_usage() -> Option<u64> {
    Some(android_alloc::ALLOCATOR.current_usage() as u64)
}

#[cfg(target_os = "windows")]
pub fn get_memory_usage() -> Option<u64> {
    use std::mem;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let handle = GetCurrentProcess();
        let mut pmc: PROCESS_MEMORY_COUNTERS = mem::zeroed();
        let cb = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

        if GetProcessMemoryInfo(handle, &mut pmc, cb) != 0 {
            return Some(pmc.WorkingSetSize as u64);
        }
    }
    None
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android",
    target_os = "windows"
)))]
pub fn get_memory_usage() -> Option<u64> {
    None
}

/// Start a background task to periodically relieve memory pressure.
/// This is especially important for Apple Network Extensions (50MB limit).
pub fn start_memory_monitor() {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;

            // Trigger macOS/iOS native memory pressure relief
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            {
                unsafe extern "C" {
                    fn malloc_zone_pressure_relief(
                        zone: *mut std::ffi::c_void,
                        delay: usize,
                    ) -> usize;
                }
                unsafe {
                    malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
                }
                tracing::debug!("Triggered malloc_zone_pressure_relief");
            }

            // Trigger Linux glibc malloc_trim
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            {
                unsafe extern "C" {
                    fn malloc_trim(pad: usize) -> i32;
                }
                unsafe {
                    malloc_trim(0);
                }
                tracing::debug!("Triggered malloc_trim");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_memory_usage() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            if let Some(usage) = get_memory_usage() {
                println!("Current memory usage: {} bytes", usage);
                assert!(usage > 0);
            } else {
                println!("Memory usage not supported on this platform");
            }
        })
        .await
        .expect("test_get_memory_usage timed out");
    }
}
