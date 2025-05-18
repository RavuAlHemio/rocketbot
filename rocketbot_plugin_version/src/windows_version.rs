use std::fmt::Write;


#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct OSVERSIONINFOEXW {
    // ULONG = u32, USHORT = u16, UCHAR = u8
    pub os_version_info_size: u32,
    pub major_version: u32,
    pub minor_version: u32,
    pub build_number: u32,
    pub platform_id: u32,
    pub csd_version: [u16; 128],
    pub service_pack_major: u16,
    pub service_pack_minor: u16,
    pub suite_mask: u16,
    pub product_type: u8,
    pub reserved: u8,
}
impl Default for OSVERSIONINFOEXW {
    fn default() -> Self {
        Self {
            os_version_info_size: Default::default(),
            major_version: Default::default(),
            minor_version: Default::default(),
            build_number: Default::default(),
            platform_id: Default::default(),
            csd_version: [0; 128],
            service_pack_major: Default::default(),
            service_pack_minor: Default::default(),
            suite_mask: Default::default(),
            product_type: Default::default(),
            reserved: Default::default(),
        }
    }
}


#[link(name = "ntdll")]
extern "C" {
    fn RtlGetVersion(version_information: *mut OSVERSIONINFOEXW) -> i32;
}

pub(crate) fn version() -> String {
    let mut buf = OSVERSIONINFOEXW::default();
    buf.os_version_info_size = std::mem::size_of_val(&buf).try_into().unwrap();

    unsafe { RtlGetVersion(&mut buf) };

    let mut ret = format!(
        "Windows {}.{}",
        buf.major_version,
        buf.minor_version,
    );
    if buf.service_pack_major > 0 {
        write!(ret, " SP {}", buf.service_pack_major).unwrap();
        if buf.service_pack_minor > 0 {
            write!(ret, ".{}", buf.service_pack_minor).unwrap();
        }
    }
    write!(ret, " Build {}", buf.build_number).unwrap();

    ret
}
