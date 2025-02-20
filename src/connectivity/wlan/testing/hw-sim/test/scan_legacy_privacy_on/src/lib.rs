// Copyright 2019 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use {
    ieee80211::{Bssid, Ssid},
    lazy_static::lazy_static,
    std::convert::TryFrom,
    wlan_common::bss::Protection,
    wlan_hw_sim::*,
};

const CHANNEL_1: u8 = 1;
const BSS_WPA1: Bssid = Bssid([0x62, 0x73, 0x73, 0x66, 0x6f, 0x6f]);
const BSS_WEP: Bssid = Bssid([0x62, 0x73, 0x73, 0x66, 0x6f, 0x72]);
const BSS_MIXED: Bssid = Bssid([0x62, 0x73, 0x73, 0x66, 0x6f, 0x7a]);

lazy_static! {
    static ref SSID_WPA1: Ssid = Ssid::try_from("wpa1___how_nice").unwrap();
    static ref SSID_WEP: Ssid = Ssid::try_from("wep_is_soooo_secure").unwrap();
    static ref SSID_MIXED: Ssid = Ssid::try_from("this_is_fine").unwrap();
}

/// Test a client can connect to a wep or wpa network only when configured on.
#[fuchsia_async::run_singlethreaded(test)]
async fn scan_legacy_privacy_on() {
    init_syslog();

    let mut helper = test_utils::TestHelper::begin_test(default_wlantap_config_client()).await;

    let () = loop_until_iface_is_found(&mut helper).await;

    let phy = helper.proxy();
    let beacons = [
        test_utils::ScanTestBeacon {
            channel: CHANNEL_1,
            bssid: BSS_WPA1,
            ssid: SSID_WPA1.clone(),
            protection: Protection::Wpa1,
            rssi: None,
        },
        test_utils::ScanTestBeacon {
            channel: CHANNEL_1,
            bssid: BSS_WEP,
            ssid: SSID_WEP.clone(),
            protection: Protection::Wep,
            rssi: None,
        },
        test_utils::ScanTestBeacon {
            channel: CHANNEL_1,
            bssid: BSS_MIXED,
            ssid: SSID_MIXED.clone(),
            protection: Protection::Wpa1Wpa2Personal,
            rssi: None,
        },
    ];
    let mut scan_results = test_utils::scan_for_networks(&phy, &beacons, &mut helper).await;
    scan_results.sort();

    let mut expected_aps = [
        (SSID_WPA1.clone(), BSS_WPA1.0, true, 0),
        (SSID_WEP.clone(), BSS_WEP.0, true, 0),
        (SSID_MIXED.clone(), BSS_MIXED.0, true, 0),
    ];
    expected_aps.sort();
    assert_eq!(&expected_aps, &scan_results[..]);
    helper.stop().await;
}
