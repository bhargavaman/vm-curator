use super::*;

#[test]
fn test_pci_device_is_gpu() {
    let device = PciDevice {
        address: "0000:01:00.0".to_string(),
        vendor_id: 0x10de,
        device_id: 0x2684,
        class_code: 0x030000,
        vendor_name: "NVIDIA".to_string(),
        device_name: "GeForce RTX 4090".to_string(),
        driver: Some("nvidia".to_string()),
        iommu_group: Some(1),
        is_boot_vga: false,
        subsystem_vendor_id: 0,
        subsystem_device_id: 0,
    };

    assert!(device.is_gpu());
    assert!(device.is_vga());
    assert!(device.is_nvidia());
    assert!(device.can_passthrough());
}

#[test]
fn test_pci_device_boot_vga_cannot_passthrough() {
    let device = PciDevice {
        address: "0000:00:02.0".to_string(),
        vendor_id: 0x8086,
        device_id: 0x1234,
        class_code: 0x030000,
        vendor_name: "Intel".to_string(),
        device_name: "UHD Graphics".to_string(),
        driver: Some("i915".to_string()),
        iommu_group: Some(0),
        is_boot_vga: true,
        subsystem_vendor_id: 0,
        subsystem_device_id: 0,
    };

    assert!(device.is_gpu());
    assert!(!device.can_passthrough());
}

#[test]
fn test_pci_bridge_is_infrastructure() {
    // IOMMU group sibling auto-inclusion (issue #25 fix) skips infrastructure
    // devices like PCI bridges. Lock that contract: PCI-to-PCI bridges (class
    // 0x060400) and host bridges (0x060000) must be classified as infrastructure
    // so they don't get added to PCI_DEVICES, which would fail vfio-pci binding.
    let pci_bridge = PciDevice {
        address: "0000:00:01.0".to_string(),
        vendor_id: 0x8086,
        device_id: 0x1901,
        class_code: 0x060400,
        vendor_name: "Intel".to_string(),
        device_name: "PCI Express Root Port".to_string(),
        driver: Some("pcieport".to_string()),
        iommu_group: Some(14),
        is_boot_vga: false,
        subsystem_vendor_id: 0,
        subsystem_device_id: 0,
    };
    assert!(pci_bridge.is_infrastructure());
    assert!(!pci_bridge.is_passthrough_candidate());
}

#[test]
fn test_generate_passthrough_args() {
    let devices = vec![
        PciDevice {
            address: "0000:01:00.0".to_string(),
            vendor_id: 0x10de,
            device_id: 0x2684,
            class_code: 0x030000,
            vendor_name: "NVIDIA".to_string(),
            device_name: "GeForce RTX 4090".to_string(),
            driver: Some("vfio-pci".to_string()),
            iommu_group: Some(1),
            is_boot_vga: false,
            subsystem_vendor_id: 0,
            subsystem_device_id: 0,
        },
        PciDevice {
            address: "0000:01:00.1".to_string(),
            vendor_id: 0x10de,
            device_id: 0x228b,
            class_code: 0x040300,
            vendor_name: "NVIDIA".to_string(),
            device_name: "HD Audio Controller".to_string(),
            driver: Some("vfio-pci".to_string()),
            iommu_group: Some(1),
            is_boot_vga: false,
            subsystem_vendor_id: 0,
            subsystem_device_id: 0,
        },
    ];

    let args = generate_passthrough_args(&devices);

    assert_eq!(args.len(), 4); // 2 devices, each with 2 args
    assert!(args[1].contains("multifunction=on")); // Primary GPU
    assert!(!args[1].contains("x-vga")); // No x-vga (incompatible with modern NVIDIA)
    assert!(!args[3].contains("x-vga")); // Audio device also no x-vga
}

/// Helper for is_integrated_gpu tests
fn gpu_device(address: &str, vendor_id: u16, device_id: u16) -> PciDevice {
    PciDevice {
        address: address.to_string(),
        vendor_id,
        device_id,
        class_code: 0x030000,
        vendor_name: String::new(),
        device_name: String::new(),
        driver: None,
        iommu_group: Some(1),
        is_boot_vga: true,
        subsystem_vendor_id: 0,
        subsystem_device_id: 0,
    }
}

#[test]
fn integrated_gpu_detection() {
    // Known AMD APUs (issue #61 reporter's Rembrandt 680M among them)
    assert!(gpu_device("0000:e4:00.0", 0x1002, 0x1681).is_integrated_gpu());
    assert!(gpu_device("0000:04:00.0", 0x1002, 0x15bf).is_integrated_gpu());

    // AMD discrete cards are not flagged
    assert!(!gpu_device("0000:03:00.0", 0x1002, 0x744c).is_integrated_gpu());

    // Intel iGPU always sits at 00:02.0; Arc dGPUs live elsewhere
    assert!(gpu_device("0000:00:02.0", 0x8086, 0x9a49).is_integrated_gpu());
    assert!(!gpu_device("0000:03:00.0", 0x8086, 0x56a0).is_integrated_gpu());

    // NVIDIA is never integrated
    assert!(!gpu_device("0000:01:00.0", 0x10de, 0x2684).is_integrated_gpu());

    // A non-GPU AMD device with a coincidental APU device ID is not flagged
    let mut audio = gpu_device("0000:e4:00.1", 0x1002, 0x1681);
    audio.class_code = 0x040300;
    assert!(!audio.is_integrated_gpu());
}

// --- Device naming: pci.ids first, corrected table as fallback (#91) ------

const PCI_IDS_FIXTURE: &str = "\
#\tList of PCI ID's (fixture)
#
1002  Advanced Micro Devices, Inc. [AMD/ATI]
\t744c  Navi 31 [Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M]
\t\t1002 0e3b  Radeon RX 7900 XTX
10de  NVIDIA Corporation
\t2486  GA104 [GeForce RTX 3060 Ti]
\t2504  GA106 [GeForce RTX 3060 Lite Hash Rate]
\t\t1458 404f  GeForce RTX 3060 (some partner card)
\t2544  GA106 [GeForce RTX 3060]
1b4b  Marvell Technology Group Ltd.
beef  Example Vendor Without Brackets
\t0001  Plain Device Name
C 03  Display controller
\t00  VGA compatible controller
";

#[test]
fn pci_ids_parse_reads_vendors_and_devices_but_not_subsystems() {
    let db = PciIdsDb::parse(PCI_IDS_FIXTURE);
    assert_eq!(db.vendor(0x10de).as_deref(), Some("NVIDIA Corporation"));
    assert_eq!(
        db.device(0x10de, 0x2504).as_deref(),
        Some("GA106 [GeForce RTX 3060 Lite Hash Rate]")
    );
    assert_eq!(
        db.device(0xbeef, 0x0001).as_deref(),
        Some("Plain Device Name")
    );
    // Subsystem lines must not be mistaken for devices, and the class
    // section must not be parsed as a vendor.
    assert!(db.device(0x10de, 0x1458).is_none());
    assert!(db.device(0x10de, 0x404f).is_none());
    assert!(db.vendor(0x0003).is_none());
    assert!(db.device(0x0003, 0x0000).is_none());
}

#[test]
fn rtx_3060_is_not_reported_as_a_3060_ti() {
    // Issue #91: 10de:2504 is a GA106 RTX 3060 (LHR); the 3060 Ti is GA104.
    let db = PciIdsDb::parse(PCI_IDS_FIXTURE);
    let (vendor, device) = get_pci_names_with(Some(&db), 0x10de, 0x2504, 0x030000);
    assert_eq!(vendor, "NVIDIA");
    assert_eq!(device, "GeForce RTX 3060 Lite Hash Rate");
    assert!(!device.contains("Ti"));

    // Without pci.ids the corrected built-in table agrees.
    let (_, fallback) = get_pci_names_with(None, 0x10de, 0x2504, 0x030000);
    assert_eq!(fallback, "GeForce RTX 3060 Lite Hash Rate");
    let (_, ti) = get_pci_names_with(None, 0x10de, 0x2486, 0x030000);
    assert_eq!(ti, "GeForce RTX 3060 Ti");
}

#[test]
fn marketing_name_prefers_bracketed_part() {
    assert_eq!(
        marketing_name("GA106 [GeForce RTX 3060 Lite Hash Rate]"),
        "GeForce RTX 3060 Lite Hash Rate"
    );
    assert_eq!(marketing_name("Plain Device Name"), "Plain Device Name");
    assert_eq!(marketing_name("Odd [] Empty"), "Odd [] Empty");
    assert_eq!(
        marketing_name("  Navi 31 [Radeon RX 7900 XT/7900 XTX]  "),
        "Radeon RX 7900 XT/7900 XTX"
    );
}

#[test]
fn pci_ids_db_takes_precedence_over_table_and_class_fallback_remains() {
    let db = PciIdsDb::parse(PCI_IDS_FIXTURE);
    // pci.ids wins over the built-in table (which says "7900 XTX / 7900 XT / 7900 GRE").
    let (vendor, device) = get_pci_names_with(Some(&db), 0x1002, 0x744c, 0x030000);
    assert_eq!(vendor, "AMD/ATI");
    assert_eq!(device, "Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M");
    // Short vendor names are kept for well-known vendors; pci.ids fills the rest.
    let (vendor, device) = get_pci_names_with(Some(&db), 0xbeef, 0x0001, 0x020000);
    assert_eq!(vendor, "Example Vendor Without Brackets");
    assert_eq!(device, "Plain Device Name");
    // Unknown everywhere: class description, empty vendor.
    let (vendor, device) = get_pci_names_with(Some(&db), 0xbeef, 0x0002, 0x020000);
    assert_eq!(vendor, "Example Vendor Without Brackets");
    assert_eq!(device, "Network Controller");
    let (vendor, device) = get_pci_names_with(None, 0xdead, 0x0002, 0x010802);
    assert_eq!(vendor, "");
    assert_eq!(device, "NVMe Controller");
}
