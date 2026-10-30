//! Process and network counters used by the memory/performance report.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct Metrics {
    pub active_connections: AtomicI64,
    pub players: AtomicI64,
    pub packets_in: AtomicU64,
    pub packets_out: AtomicU64,
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub outbound_queued: AtomicI64,
    pub rejected_connections: AtomicU64,
    pub map_frames: AtomicU64,
    pub map_bytes: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct Snapshot {
    pub rss_bytes: Option<u64>,
    pub active_connections: i64,
    pub players: i64,
    pub packets_in: u64,
    pub packets_out: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub outbound_queued: i64,
    pub rejected_connections: u64,
    pub map_frames: u64,
    pub map_bytes: u64,
}

impl Metrics {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            rss_bytes: rss_bytes(),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            players: self.players.load(Ordering::Relaxed),
            packets_in: self.packets_in.load(Ordering::Relaxed),
            packets_out: self.packets_out.load(Ordering::Relaxed),
            bytes_in: self.bytes_in.load(Ordering::Relaxed),
            bytes_out: self.bytes_out.load(Ordering::Relaxed),
            outbound_queued: self.outbound_queued.load(Ordering::Relaxed),
            rejected_connections: self.rejected_connections.load(Ordering::Relaxed),
            map_frames: self.map_frames.load(Ordering::Relaxed),
            map_bytes: self.map_bytes.load(Ordering::Relaxed),
        }
    }
}

/// Resident set size of this process.
#[cfg(target_os = "linux")]
pub fn rss_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages: u64 = text.split_whitespace().nth(1)?.parse().ok()?;
    // Linux reports resident pages. The page size is almost always 4096; use
    // sysconf when available would add a dependency, so keep the documented
    // assumption here.
    Some(resident_pages * 4096)
}

#[cfg(windows)]
pub fn rss_bytes() -> Option<u64> {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCounters {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }

    extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut core::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: core::mem::size_of::<ProcessMemoryCounters>() as u32,
        PageFaultCount: 0,
        PeakWorkingSetSize: 0,
        WorkingSetSize: 0,
        QuotaPeakPagedPoolUsage: 0,
        QuotaPagedPoolUsage: 0,
        QuotaPeakNonPagedPoolUsage: 0,
        QuotaNonPagedPoolUsage: 0,
        PagefileUsage: 0,
        PeakPagefileUsage: 0,
    };
    // SAFETY: the process handle is the current pseudo-handle (always valid),
    // and `counters` points to a correctly sized, initialised structure.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    (ok != 0).then_some(counters.WorkingSetSize as u64)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn rss_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reports_counters() {
        let metrics = Metrics::default();
        metrics.players.store(3, Ordering::Relaxed);
        let snap = metrics.snapshot();
        assert_eq!(snap.players, 3);
    }
}
