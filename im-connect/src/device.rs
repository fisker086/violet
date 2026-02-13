//! 设备类型与分组，与 Lucky-cloud IMDeviceType 对齐

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceGroup {
    Mobile,
    Desktop,
    Web,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IMDeviceType {
    pub type_name: &'static str,
    pub group: DeviceGroup,
}

impl IMDeviceType {
    pub const ANDROID: Self = Self { type_name: "android", group: DeviceGroup::Mobile };
    pub const IOS: Self = Self { type_name: "ios", group: DeviceGroup::Mobile };
    pub const WEB: Self = Self { type_name: "web", group: DeviceGroup::Web };
    pub const MAC: Self = Self { type_name: "mac", group: DeviceGroup::Desktop };
    pub const WIN: Self = Self { type_name: "win", group: DeviceGroup::Desktop };
    pub const LINUX: Self = Self { type_name: "linux", group: DeviceGroup::Desktop };

    const ALL: [Self; 6] = [Self::ANDROID, Self::IOS, Self::WEB, Self::MAC, Self::WIN, Self::LINUX];

    pub fn of(s: &str) -> Option<Self> {
        let key = s.trim().to_lowercase();
        if key.is_empty() {
            return None;
        }
        Self::ALL.iter().find(|t| t.type_name == key).copied()
    }

    pub fn of_or_default(s: &str, default: Self) -> Self {
        Self::of(s).unwrap_or(default)
    }

    #[allow(dead_code)]
    pub fn group_from(s: &str, default: DeviceGroup) -> DeviceGroup {
        Self::of(s).map(|t| t.group).unwrap_or(default)
    }

    #[allow(dead_code)]
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }
}

impl FromStr for DeviceGroup {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_uppercase().as_str() {
            "MOBILE" => Ok(DeviceGroup::Mobile),
            "DESKTOP" => Ok(DeviceGroup::Desktop),
            "WEB" => Ok(DeviceGroup::Web),
            _ => Err(()),
        }
    }
}
