pub fn settings_device_head_root_id(device_id: &str) -> String {
    format!("settings/devices/{device_id}/head")
}

pub fn sync_membership_record_root_id(record_id: &str) -> String {
    format!("account/membership/{record_id}")
}

#[cfg(test)]
mod tests {
    use super::{settings_device_head_root_id, sync_membership_record_root_id};

    #[test]
    fn settings_device_head_root_id_formats_per_device_head() {
        assert_eq!(
            settings_device_head_root_id("device-a"),
            "settings/devices/device-a/head"
        );
    }

    #[test]
    fn sync_membership_record_root_id_formats_membership_records() {
        assert_eq!(
            sync_membership_record_root_id("epoch-1-enroll-device-a"),
            "account/membership/epoch-1-enroll-device-a"
        );
    }
}
