//! Minimal runtime binding to Npcap's wpcap.dll (no SDK / import library needed; the app starts without Npcap).
use anyhow::{anyhow, bail, Result};
use libloading::Library;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::sync::Arc;

#[repr(C)]
struct PcapIf {
    next: *mut PcapIf,
    name: *const c_char,
    description: *const c_char,
    addresses: *mut PcapAddr,
    flags: c_uint,
}
#[repr(C)]
struct PcapAddr {
    next: *mut PcapAddr,
    addr: *const u16, // sockaddr: first field is the family
    _rest: [*const c_void; 3],
}
#[repr(C)]
struct PktHdr {
    ts_sec: i32,
    ts_usec: i32,
    caplen: c_uint,
    len: c_uint,
}
#[repr(C)]
struct BpfProgram {
    len: c_uint,
    insns: *mut c_void,
}

type FindAll = unsafe extern "C" fn(*mut *mut PcapIf, *mut c_char) -> c_int;
type FreeAll = unsafe extern "C" fn(*mut PcapIf);
type OpenLive = unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, *mut c_char) -> *mut c_void;
type Compile = unsafe extern "C" fn(*mut c_void, *mut BpfProgram, *const c_char, c_int, c_uint) -> c_int;
type SetFilter = unsafe extern "C" fn(*mut c_void, *mut BpfProgram) -> c_int;
type NextEx = unsafe extern "C" fn(*mut c_void, *mut *mut PktHdr, *mut *const u8) -> c_int;
type Datalink = unsafe extern "C" fn(*mut c_void) -> c_int;
type Close = unsafe extern "C" fn(*mut c_void);

pub struct Pcap {
    lib: Library,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub description: String,
}

pub struct Handle {
    api: Arc<Pcap>,
    h: *mut c_void,
    pub datalink: i32,
}
unsafe impl Send for Handle {}

pub enum Next<'a> {
    Packet(&'a [u8]),
    Timeout,
    Closed,
}

impl Pcap {
    /// Load wpcap.dll from the Npcap folder (or PATH). Error text is meant for the user.
    pub fn load() -> Result<Arc<Pcap>> {
        #[cfg(windows)]
        {
            use libloading::os::windows::{Library as WLib, LOAD_WITH_ALTERED_SEARCH_PATH};
            let sys = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
            let p = format!("{sys}\\System32\\Npcap\\wpcap.dll");
            if std::path::Path::new(&p).exists() {
                let lib = unsafe { WLib::load_with_flags(&p, LOAD_WITH_ALTERED_SEARCH_PATH) }.map_err(|e| anyhow!("{p}: {e}"))?;
                return Ok(Arc::new(Pcap { lib: lib.into() }));
            }
        }
        let lib = unsafe { Library::new(if cfg!(windows) { "wpcap.dll" } else { "libpcap.so.0.8" }) }
            .map_err(|_| anyhow!("Npcap is not installed (wpcap.dll not found)"))?;
        Ok(Arc::new(Pcap { lib }))
    }

    /// Non-loopback devices that have at least one IP address.
    pub fn devices(&self) -> Result<Vec<Device>> {
        unsafe {
            let find: libloading::Symbol<FindAll> = self.lib.get(b"pcap_findalldevs\0")?;
            let free: libloading::Symbol<FreeAll> = self.lib.get(b"pcap_freealldevs\0")?;
            let mut all: *mut PcapIf = std::ptr::null_mut();
            let mut err = [0 as c_char; 256];
            if find(&mut all, err.as_mut_ptr()) != 0 {
                bail!("pcap_findalldevs: {}", CStr::from_ptr(err.as_ptr()).to_string_lossy());
            }
            let mut out = vec![];
            let mut d = all;
            while !d.is_null() {
                let dev = &*d;
                let loopback = dev.flags & 1 != 0;
                let mut has_ip = false;
                let mut a = dev.addresses;
                while !a.is_null() {
                    if !(*a).addr.is_null() && matches!(*(*a).addr, 2 | 23) {
                        has_ip = true;
                    }
                    a = (*a).next;
                }
                if !loopback && has_ip {
                    let s = |p: *const c_char| if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() };
                    out.push(Device { name: s(dev.name), description: s(dev.description) });
                }
                d = dev.next;
            }
            free(all);
            Ok(out)
        }
    }

    pub fn open(self: &Arc<Self>, dev: &str, filter: &str) -> Result<Handle> {
        unsafe {
            let open: libloading::Symbol<OpenLive> = self.lib.get(b"pcap_open_live\0")?;
            let compile: libloading::Symbol<Compile> = self.lib.get(b"pcap_compile\0")?;
            let setf: libloading::Symbol<SetFilter> = self.lib.get(b"pcap_setfilter\0")?;
            let dl: libloading::Symbol<Datalink> = self.lib.get(b"pcap_datalink\0")?;
            let close: libloading::Symbol<Close> = self.lib.get(b"pcap_close\0")?;
            let name = CString::new(dev)?;
            let mut err = [0 as c_char; 256];
            let h = open(name.as_ptr(), 65535, 0, 100, err.as_mut_ptr());
            if h.is_null() {
                bail!("{}", CStr::from_ptr(err.as_ptr()).to_string_lossy());
            }
            let mut prog = BpfProgram { len: 0, insns: std::ptr::null_mut() };
            let f = CString::new(filter)?;
            if compile(h, &mut prog, f.as_ptr(), 1, 0xffff_ffff) != 0 || setf(h, &mut prog) != 0 {
                close(h);
                bail!("cannot set capture filter");
            }
            Ok(Handle { api: self.clone(), h, datalink: dl(h) })
        }
    }
}

impl Handle {
    pub fn next(&mut self) -> Next<'_> {
        unsafe {
            let Ok(next) = self.api.lib.get::<NextEx>(b"pcap_next_ex\0") else { return Next::Closed };
            let (mut hdr, mut data): (*mut PktHdr, *const u8) = (std::ptr::null_mut(), std::ptr::null());
            match next(self.h, &mut hdr, &mut data) {
                1 if !hdr.is_null() && !data.is_null() => Next::Packet(std::slice::from_raw_parts(data, (*hdr).caplen as usize)),
                0 | 1 => Next::Timeout,
                _ => Next::Closed,
            }
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            if let Ok(close) = self.api.lib.get::<Close>(b"pcap_close\0") {
                close(self.h);
            }
        }
    }
}
