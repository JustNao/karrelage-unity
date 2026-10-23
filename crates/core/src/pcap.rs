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
type FreeCode = unsafe extern "C" fn(*mut BpfProgram);

/// The wpcap entry points, resolved once at load (not once per packet).
struct Api {
    find: FindAll,
    free: FreeAll,
    open: OpenLive,
    compile: Compile,
    setfilter: SetFilter,
    next: NextEx,
    datalink: Datalink,
    close: Close,
    freecode: FreeCode,
}

pub struct Pcap {
    /// keeps the DLL loaded for as long as the function pointers of `api` are used
    _lib: Library,
    api: Api,
}

/// `pcap_compile` is not thread-safe in older libpcap releases: one compilation at a time.
static COMPILING: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub description: String,
}

pub struct Handle {
    pcap: Arc<Pcap>,
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
    /// Load wpcap.dll from the Npcap folder (or System32 for Npcap's WinPcap-compatible mode). Error text is meant for the user.
    pub fn load() -> Result<Arc<Pcap>> {
        #[cfg(windows)]
        {
            use libloading::os::windows::{Library as WLib, LOAD_WITH_ALTERED_SEARCH_PATH};
            let sys = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
            // absolute paths only: a wpcap.dll planted next to the app or on PATH must never be loaded
            for p in [format!("{sys}\\System32\\Npcap\\wpcap.dll"), format!("{sys}\\System32\\wpcap.dll")] {
                if std::path::Path::new(&p).exists() {
                    let lib = unsafe { WLib::load_with_flags(&p, LOAD_WITH_ALTERED_SEARCH_PATH) }.map_err(|e| anyhow!("{p}: {e}"))?;
                    return Self::bind(lib.into());
                }
            }
            bail!("Npcap is not installed (wpcap.dll not found)");
        }
        #[cfg(not(windows))]
        {
            let lib = unsafe { Library::new("libpcap.so.0.8") }.map_err(|_| anyhow!("libpcap is not installed"))?;
            Self::bind(lib)
        }
    }

    fn bind(lib: Library) -> Result<Arc<Pcap>> {
        unsafe {
            let api = Api {
                find: *lib.get::<FindAll>(b"pcap_findalldevs\0")?,
                free: *lib.get::<FreeAll>(b"pcap_freealldevs\0")?,
                open: *lib.get::<OpenLive>(b"pcap_open_live\0")?,
                compile: *lib.get::<Compile>(b"pcap_compile\0")?,
                setfilter: *lib.get::<SetFilter>(b"pcap_setfilter\0")?,
                next: *lib.get::<NextEx>(b"pcap_next_ex\0")?,
                datalink: *lib.get::<Datalink>(b"pcap_datalink\0")?,
                close: *lib.get::<Close>(b"pcap_close\0")?,
                freecode: *lib.get::<FreeCode>(b"pcap_freecode\0")?,
            };
            Ok(Arc::new(Pcap { _lib: lib, api }))
        }
    }

    /// Non-loopback devices that have at least one IP address.
    pub fn devices(&self) -> Result<Vec<Device>> {
        let (find, free) = (self.api.find, self.api.free);
        unsafe {
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
        let name = CString::new(dev)?;
        let mut err = [0 as c_char; 256];
        let h = unsafe { (self.api.open)(name.as_ptr(), 65535, 0, 100, err.as_mut_ptr()) };
        if h.is_null() {
            bail!("{}", unsafe { CStr::from_ptr(err.as_ptr()) }.to_string_lossy());
        }
        // from here on, dropping the handle closes it
        let mut handle = Handle { pcap: self.clone(), h, datalink: unsafe { (self.api.datalink)(h) } };
        handle.set_filter(filter)?;
        Ok(handle)
    }
}

impl Handle {
    /// Replace the BPF filter of an open handle (the driver drops what it rejects, before any copy to the app).
    pub fn set_filter(&mut self, filter: &str) -> Result<()> {
        let api = &self.pcap.api;
        let f = CString::new(filter)?;
        let mut prog = BpfProgram { len: 0, insns: std::ptr::null_mut() };
        let compiled = {
            let _one = COMPILING.lock().unwrap_or_else(|e| e.into_inner());
            unsafe { (api.compile)(self.h, &mut prog, f.as_ptr(), 1, 0xffff_ffff) == 0 }
        };
        if !compiled {
            bail!("cannot compile capture filter {filter:?}");
        }
        let set = unsafe { (api.setfilter)(self.h, &mut prog) == 0 };
        unsafe { (api.freecode)(&mut prog) };
        if !set {
            bail!("cannot set capture filter");
        }
        Ok(())
    }

    pub fn next(&mut self) -> Next<'_> {
        unsafe {
            let (mut hdr, mut data): (*mut PktHdr, *const u8) = (std::ptr::null_mut(), std::ptr::null());
            match (self.pcap.api.next)(self.h, &mut hdr, &mut data) {
                1 if !hdr.is_null() && !data.is_null() => Next::Packet(std::slice::from_raw_parts(data, (*hdr).caplen as usize)),
                0 | 1 => Next::Timeout,
                _ => Next::Closed,
            }
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { (self.pcap.api.close)(self.h) }
    }
}
