use std::env;

#[derive(Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Unsupported,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Arch {
    Amd64,
    Arm64,
    Unsupported,
}

pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

pub fn get_platform_info() -> Platform {
    let os = match env::consts::OS {
        "linux" => Os::Linux,
        "macos" => Os::MacOs,
        _ => Os::Unsupported,
    };

    let arch = match env::consts::ARCH {
        "x86_64" => Arch::Amd64,
        "aarch64" => Arch::Arm64,
        _ => Arch::Unsupported,
    };

    Platform { os, arch }
}

impl ToString for Platform {
    fn to_string(&self) -> String {
        let os_str = match self.os {
            Os::Linux => "linux",
            Os::MacOs => "darwin", // Keep consistency with shell script output 'darwin' for macOS
            Os::Unsupported => "unsupported_os",
        };
        let arch_str = match self.arch {
            Arch::Amd64 => "amd64",
            Arch::Arm64 => "arm64",
            Arch::Unsupported => "unsupported_arch",
        };
        format!("{}_{}", os_str, arch_str)
    }
}
