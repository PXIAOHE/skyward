use anyhow::{anyhow, Result};
use skyward_config::DeviceConfig;
use skyward_info::{Battery, Boot, Charge, Cpu, DeviceInfo, PatchLevel, Storage};
use skyward_install::{Failure, ImageKind, Outcome, Ramdisk, Reflash, Refusal, Stage};
use skyward_ui::app::{
    Action, App, Asked, BackupAt, BackupEnd, BackupEntry, BackupJob, BackupRefusal, Choice, Clip, Damage, Data, EraseStop, FileKind,
    FileOp, FileRow, FileTask, Held, Install, JobEnd, Key, Keys, MountFailure, MountRow, Part, Pick, Place, Screen, Source, Touch,
    UsbDrive, UsbLink, SCREEN_OFF,
};
use skyward_ui::widgets::{self, Bar, Layer};
use skyward_ui::{fmt, pages, Canvas, Ctx, Fonts, Theme};
use skyward_vab::{BootSlots, Hold, Slot, SnapshotState};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

mod matcher;

const CLOCK: &str = "23:46";

/// What a shell in the recovery writes for a few commands, colours and all.
const SHELL_SAMPLE: &[u8] = b"ishtar:/ # getprop ro.product.device\r\nishtar\r\n\
ishtar:/ # ls --color=always /\r\n\
\x1b[1;34macct\x1b[0m  \x1b[1;34mapex\x1b[0m  \x1b[1;36mbin\x1b[0m  \x1b[1;34mconfig\x1b[0m  \x1b[1;34mdata\x1b[0m  \x1b[1;34mdev\x1b[0m\r\n\
\x1b[1;36metc\x1b[0m  \x1b[1;32minit\x1b[0m  \x1b[1;34mmetadata\x1b[0m  \x1b[1;34mmnt\x1b[0m  \x1b[1;34modm\x1b[0m\r\n\
\x1b[1;34mproc\x1b[0m  \x1b[1;34msys\x1b[0m  \x1b[1;34msystem\x1b[0m  \x1b[1;34mtmp\x1b[0m  \x1b[1;34mvendor\x1b[0m\r\n\
ishtar:/ # df -h /metadata\r\n\
Filesystem      Size Used Avail Use% Mounted on\r\n\
/dev/block/sda9  59M 7.0M   52M  12% /metadata\r\n\
ishtar:/ # ";

/// File name, page, dark theme, readings, where the data partition stands,
/// pattern drawn.
type Shot<'a> = (&'a str, Screen, bool, &'a DeviceInfo, Data, &'a [usize]);

/// File name, dark theme, readings, a mount that failed, and the partitions
/// left unmounted.
type MountShot<'a> = (&'a str, bool, &'a DeviceInfo, Option<MountFailure>, &'a [&'a str]);

/// File name, page, dark theme, and what to set before the page is drawn.
type PageShot<'a> = (&'a str, Screen, bool, &'a dyn Fn(&mut App));

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("preview-out"));
    std::fs::create_dir_all(&out)?;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let font_path = font_path()?;
    let fonts = Fonts::load(&font_path, 0, root.join("resources/fonts/KirkMono-Regular.ttf"))?;
    eprintln!("font: {}", font_path.display());
    for (tag, min, def, max) in fonts.axes() {
        eprintln!("axis {tag}: {min}..{max} (default {def})");
    }

    let cfg = DeviceConfig::load(root.join("devices/nezha.toml"))?;
    let (w, h) = (cfg.display.width, cfg.display.height);
    let ppd = cfg.display.density as f32 / 160.0;
    let light = Theme::light(w, h, ppd);

    match args.get(2).map(String::as_str) {
        Some("--calibrate") => {
            let path = out.join("calibrate.png");
            write_png(&path, &calibrate(&fonts, &light)?)?;
            println!("wrote {}", path.display());
            return Ok(());
        }
        Some("--match") => {
            let probes = args.get(3).ok_or_else(|| anyhow!("--match needs a probe file"))?;
            return matcher::run(&fonts, Path::new(probes));
        }
        _ => {}
    }

    let info = nezha(&cfg);
    let pending = DeviceInfo { snapshot: SnapshotState::Snapshotted, ..info.clone() };
    let pattern = Data::Locked { user: 0, keys: Keys::Pattern, checking: false };
    // Whole pages, the way a long screenshot shows them. The About page's
    // field is drawn only on screen, so it appears among the moments below.
    let shots: [Shot; 12] = [
        ("01-decrypt-dark", Screen::Decrypt, true, &info, pattern, &[0, 3, 4, 5, 8]),
        ("02-home-light", Screen::Home, false, &info, Data::Open, &[]),
        ("03-home-dark-update-pending", Screen::Home, true, &pending, Data::Open, &[]),
        ("04-settings-light", Screen::Settings, false, &info, Data::Open, &[]),
        ("05-settings-dark", Screen::Settings, true, &info, Data::Open, &[]),
        ("06-reboot-dark", Screen::Reboot, true, &info, Data::Open, &[]),
        ("07-wipe-light", Screen::Wipe, false, &info, Data::Open, &[]),
        ("09-specs-light", Screen::Specs, false, &info, Data::Open, &[]),
        ("10-specs-dark", Screen::Specs, true, &info, Data::Open, &[]),
        ("11-install-light", Screen::Install, false, &info, Data::Open, &[]),
        ("12-install-dark-update-pending", Screen::Install, true, &pending, Data::Open, &[]),
        ("13-screen-off-light", Screen::ScreenOff, false, &info, Data::Open, &[]),
    ];
    for (name, screen, dark, info, data, pattern) in shots {
        let mut app = App::new(screen, w, h, ppd);
        app.state.dark = dark;
        app.state.data = data;
        app.state.pattern = pattern.to_vec();
        let path = out.join(format!("{name}.png"));
        write_png(&path, &whole_page(&mut app, &fonts, info))?;
        println!("wrote {}", path.display());
    }

    // The screen kept on, which warns about burn-in.
    let mut app = opened(Screen::ScreenOff, w, h, ppd);
    app.state.dark = true;
    app.state.screen_off = SCREEN_OFF.iter().position(Option::is_none).expect("one choice keeps the screen on");
    let path = out.join("14-screen-off-never-dark.png");
    write_png(&path, &whole_page(&mut app, &fonts, &info))?;
    println!("wrote {}", path.display());

    // The sideload page through an install, as the installer tells it.
    let recovery = ImageKind::Boot { kernel: false, ramdisk: Ramdisk::Recovery };
    let installing: [(&str, bool, Install); 13] = [
        ("30-sideload-waiting", false, Install::Waiting),
        ("31-sideload-checking", false, Install::Checking { size: 7_103_043_868 }),
        (
            "32-sideload-choosing",
            false,
            Install::Choosing(Choice { kind: recovery, size: 43_257_856, partitions: vec!["recovery"], partition: 0, pick: Pick::Current }),
        ),
        (
            "33-sideload-writing-dark",
            true,
            Install::Updating {
                version: Some("OS4.0.5.0.XPACNXM".into()),
                stage: Some(Stage::Writing { done: 2_934_000_000, total: 7_103_041_280 }),
                overwriting: true,
            },
        ),
        (
            "34-sideload-verifying",
            false,
            Install::Updating {
                version: Some("OS4.0.5.0.XPACNXM".into()),
                stage: Some(Stage::Verifying { index: 6, count: 21, partition: "system".into() }),
                overwriting: false,
            },
        ),
        ("35-sideload-flashing", false, Install::Flashing { partition: "boot", stage: Some(Stage::Flashing { done: 300, total: 800 }) }),
        ("36-sideload-installed", false, Install::Finished { outcome: Outcome::Installed { slot: Slot::A, wipe: false }, damage: None }),
        ("96-sideload-light-wipes", false, Install::Confirming { version: Some("OS3.0.306.0.WMACNXM".into()), wipe: true }),
        ("18-install-ready", false, Install::Confirming { version: Some("OS3.0.306.0.WMACNXM".into()), wipe: false }),
        (
            "37-sideload-flashed-recovery-dark",
            true,
            Install::Finished { outcome: Outcome::Flashed { partition: "recovery", slots: vec![Slot::A, Slot::B] }, damage: None },
        ),
        (
            "38-sideload-refused",
            false,
            Install::Finished { outcome: Outcome::Refused(Refusal::OtherDevice("ishtar".into())), damage: None },
        ),
        (
            "39-sideload-failed",
            false,
            Install::Finished { outcome: Outcome::Failed(Failure::Engine(9)), damage: Some(Damage::System) },
        ),
        (
            "40-sideload-error-dark",
            true,
            Install::Finished {
                outcome: Outcome::Failed(Failure::Error("set USB up for sideload: open ep0: Device or resource busy (os error 16)".into())),
                damage: None,
            },
        ),
    ];
    for (name, dark, install) in installing {
        let mut app = opened(Screen::Installing, w, h, ppd);
        app.state.dark = dark;
        app.state.install = install;
        let path = out.join(format!("{name}.png"));
        write_png(&path, &whole_page(&mut app, &fonts, &info))?;
        println!("wrote {}", path.display());
    }

    // The mount page as it opens after boot, after an unmount that failed,
    // and while an update waits to merge, with some partitions unmounted.
    let busy = MountFailure {
        mounting: false,
        base: "vendor".into(),
        detail: "unmount /vendor: Device or resource busy (os error 16)".into(),
    };
    let mounting: [MountShot; 3] = [
        ("15-mount-light", false, &info, None, &[]),
        ("16-mount-dark-unmount-failed", true, &info, Some(busy), &[]),
        ("17-mount-light-update-pending", false, &pending, None, &["product", "system_ext"]),
    ];
    for (name, dark, info, failure, unmounted) in mounting {
        let mut app = opened(Screen::Mount, w, h, ppd);
        app.state.dark = dark;
        app.set_mounts(Ok(sample_mounts(unmounted)));
        if let Some(failure) = failure {
            app.mount_failed(failure);
        }
        let path = out.join(format!("{name}.png"));
        write_png(&path, &whole_page(&mut app, &fonts, info))?;
        println!("wrote {}", path.display());
    }

    // The pages the remaining features open, each with what it would show.
    let made = |place, made, size, parts, system: &str| {
        let held = Held { made, size, parts, device: cfg.device.codename.clone(), system: Some(system.into()), slot: Some(Slot::B) };
        BackupEntry { place, name: fmt::stamp(made, 0), held: Ok(held) }
    };
    let backups = vec![
        made(Place::Storage, 1_792_348_260, 6_214_000_000, [true, true, false, true], "OS4.0.4.0.XPACNXM"),
        made(Place::UsbDrive, 1_791_830_000, 141_000_000, [true, false, false, false], "OS4.0.3.0.XPACNXM"),
        BackupEntry {
            place: Place::Storage,
            name: "20261020-221500".into(),
            held: Err("read /sdcard/Skyward/backups/20261020-221500/backup.toml: No such file or directory (os error 2)".into()),
        },
    ];
    // What a backup of the slot's partitions and the baseband goes through.
    let plan = || -> Vec<(String, u64)> {
        [
            ("boot", 100_663_296),
            ("init_boot", 8_388_608),
            ("vendor_boot", 100_663_296),
            ("dtbo", 25_165_824),
            ("vbmeta", 65_536),
            ("vbmeta_system", 65_536),
            ("recovery", 104_857_600),
            ("system", 1_253_212_160),
            ("system_ext", 702_984_192),
            ("product", 4_312_031_232),
            ("vendor", 2_101_317_632),
            ("odm", 1_420_455_936),
            ("vendor_dlkm", 104_177_664),
            ("system_dlkm", 26_177_536),
            ("modemst1", 2_097_152),
            ("modemst2", 2_097_152),
            ("fsg", 8_388_608),
            ("fsc", 131_072),
            ("persist", 67_108_864),
        ]
        .map(|(name, size)| (name.to_string(), size))
        .to_vec()
    };
    let job = |restore, plan: Vec<(String, u64)>, at, outcome| {
        Some(BackupJob { restore, place: Place::Storage, checks: true, plan, at, outcome, stopping: false })
    };
    let reset_asked = || Asked { reason: Some("factroy_reset(1217)".into()), ..Asked::default() };
    let in_download = |app: &mut App, name: &str| {
        app.set_files("/sdcard/Download".into(), Ok(sample_download()), LISTED);
        app.state.file = sample_download().into_iter().find(|e| e.name == name);
    };
    let ota = "/sdcard/Download/OTA_ishtar_OS3.0.305.0.WMACNXM_20260412_full_package_signed_for_recovery_install.zip";
    let copying = || FileTask::Copy { path: ota.into(), into: "/sdcard/Skyward".into() };
    let from_storage = |app: &mut App, install: Install| {
        app.state.source = Source::File(Place::Storage);
        app.state.install = install;
    };
    // What a ROM's install script shows as it runs, in the ways scripts lay
    // their lines out; the failing one stops on writing vendor.
    let said = |failed: bool| -> Vec<String> {
        let mut lines = vec![
            " ",
            "******************************",
            "  Example ROM 2.1 for ishtar",
            "******************************",
            " ",
            "- 正在检查设备型号",
            "- Mounting system and vendor",
            "- Flashing boot to /dev/block/by-name/boot_b",
            "- Flashing dtbo and vendor_boot to the inactive slot, which takes a few seconds on most devices",
            "  Partition      Size       State",
            "  system_b       1.17 GiB   written",
        ];
        lines.extend(match failed {
            false => ["  vendor_b       1.96 GiB   written", "- Cleaning up", "- Done"].as_slice(),
            true => ["! Could not write vendor_b", "! Installation aborted"].as_slice(),
        });
        lines.into_iter().map(String::from).collect()
    };
    let script = |app: &mut App, install: Install, failed: bool| {
        from_storage(app, install);
        app.state.package_name = Some("ExampleROM-2.1-ishtar.zip".into());
        app.state.script_output = said(failed);
    };
    let usb_page = Screen::Choose(Place::UsbDrive);
    let pages: [PageShot; 90] = [
        ("11-install-light-locked", Screen::Install, false, &|app| {
            app.state.data = Data::Locked { user: 0, keys: Keys::Digits, checking: false };
        }),
        ("11-install-dark-usb", Screen::Install, true, &|app| {
            app.set_usb_drive(Some(UsbDrive::Shown));
        }),
        ("19-choose-usb-light-absent", usb_page, false, &|_| {}),
        ("19-choose-usb-dark-unreadable", usb_page, true, &|app| {
            let why = "sdg holds no filesystem the kernel reads: sdg1 as vfat: Invalid argument (os error 22); sdg1 as exfat: Invalid argument (os error 22); sdg1 as ntfs3: No such device (os error 19); sdg1 as ext4: Invalid argument (os error 22); sdg1 as f2fs: Invalid argument (os error 22)";
            app.set_usb_drive(Some(UsbDrive::Unreadable(why.into())));
        }),
        ("19-choose-usb-light", usb_page, false, &|app| {
            app.set_usb_drive(Some(UsbDrive::Shown));
            let made = 1_790_380_800;
            let rows = vec![folder("ROMs", 4, made), file("boot.img", 100_663_296, made), file("ishtar-ota-OS3.0.306.0.WMACNXM.zip", 7_103_043_868, made)];
            app.set_place(Place::UsbDrive, String::new(), Ok(rows), LISTED);
        }),
        ("20-install-usb-checking", Screen::Installing, false, &|app| {
            app.state.source = Source::File(Place::UsbDrive);
            app.state.install = Install::Checking { size: 7_103_043_868 };
        }),
        ("19-choose-storage-light", Screen::Choose(Place::Storage), false, &|app| {
            app.set_place(Place::Storage, "Download".into(), Ok(sample_download()), LISTED)
        }),
        ("19-choose-storage-dark-empty", Screen::Choose(Place::Storage), true, &|app| {
            app.set_place(Place::Storage, "Download/Skyward".into(), Ok(vec![file("notes.txt", 2_048, LISTED - 600)]), LISTED)
        }),
        ("20-install-storage-checking", Screen::Installing, false, &|app| {
            from_storage(app, Install::Checking { size: 7_014_220_000 })
        }),
        ("20-install-storage-writing-dark", Screen::Installing, true, &|app| {
            let stage = Some(Stage::Writing { done: 2_934_000_000, total: 7_014_220_000 });
            from_storage(app, Install::Updating { version: Some("OS3.0.305.0.WMACNXM".into()), stage, overwriting: false })
        }),
        ("20-install-storage-refused", Screen::Installing, false, &|app| {
            from_storage(app, Install::Finished { outcome: Outcome::Refused(Refusal::Unknown), damage: None })
        }),
        ("20-install-storage-failed", Screen::Installing, false, &|app| {
            let gone = "read /sdcard/Download/OTA_ishtar_OS3.0.305.0.WMACNXM.zip: Input/output error (os error 5)";
            from_storage(app, Install::Finished { outcome: Outcome::Failed(Failure::Unreadable(gone.into())), damage: Some(Damage::System) })
        }),
        ("20-install-script-confirming", Screen::Installing, false, &|app| {
            script(app, Install::ConfirmingScript, false);
            app.state.script_output.clear();
        }),
        ("20-install-script-running-dark", Screen::Installing, true, &|app| {
            script(app, Install::Scripting { stage: Some(Stage::Script { thousandths: 420 }), running: true }, false);
            app.state.script_output.truncate(9);
        }),
        ("20-install-script-ran", Screen::Installing, false, &|app| {
            script(app, Install::Finished { outcome: Outcome::Ran { wipe_cache: false }, damage: None }, false)
        }),
        ("20-install-script-failed-dark", Screen::Installing, true, &|app| {
            script(app, Install::Finished { outcome: Outcome::Failed(Failure::Script(1)), damage: Some(Damage::Script) }, true)
        }),
        ("20-install-script-reflashing", Screen::Installing, false, &|app| {
            let stage = Some(Stage::Reflashing { done: 52_428_800, total: 209_715_200 });
            script(app, Install::Scripting { stage, running: true }, false)
        }),
        ("20-install-script-ran-reflashed-dark", Screen::Installing, true, &|app| {
            script(app, Install::Finished { outcome: Outcome::Ran { wipe_cache: false }, damage: None }, false);
            app.state.reflash = Some(Reflash::Written(vec![Slot::A, Slot::B]));
        }),
        ("20-install-script-reflash-failed", Screen::Installing, false, &|app| {
            script(app, Install::Finished { outcome: Outcome::Failed(Failure::Script(1)), damage: Some(Damage::Script) }, true);
            let why = "write /dev/block/by-name/recovery_a: Input/output error (os error 5)";
            app.state.reflash = Some(Reflash::Failed(why.into()));
        }),
        ("20-install-storage-installed-reflashed", Screen::Installing, false, &|app| {
            from_storage(app, Install::Finished { outcome: Outcome::Installed { slot: Slot::A, wipe: false }, damage: None });
            app.state.reflash = Some(Reflash::Written(vec![Slot::A]));
        }),
        ("11-install-light-recovery-partitions", Screen::Install, false, &|app| app.state.recovery_partitions = true),
        ("41-search-light", Screen::Search, false, &|_| {}),
        ("42-search-light-found", Screen::Search, false, &|app| app.state.search = "b".into()),
        ("43-search-dark-pinyin", Screen::Search, true, &|app| app.state.search = "zd".into()),
        ("44-backup-light", Screen::Backup, false, &|_| {}),
        ("45-backup-dark", Screen::Backup, true, &|app| {
            app.set_usb_drive(Some(UsbDrive::Shown));
            app.state.backup_to = Place::UsbDrive;
        }),
        ("46-restore-light-none", Screen::Restore, false, &|_| {}),
        ("47-restore-light", Screen::Restore, false, &|app| app.set_backups(backups.clone())),
        ("48-restore-backup-light", Screen::RestoreBackup, false, &|app| {
            app.state.restore = Some((backups[0].clone(), [true, false, false, true]));
        }),
        ("48-restore-backup-dark-deleting", Screen::RestoreBackup, true, &|app| {
            app.state.restore = Some((backups[0].clone(), [true, true, false, true]));
            app.state.asking_delete = true;
        }),
        ("48-restore-backup-light-other-version", Screen::RestoreBackup, false, &|app| {
            app.state.restore = Some((backups[1].clone(), [true, false, false, false]));
        }),
        ("48-restore-backup-light-unreadable", Screen::RestoreBackup, false, &|app| {
            app.state.restore = Some((backups[2].clone(), [false; 4]));
        }),
        ("100-backup-job-light-measuring", Screen::BackupJob, false, &|app| app.state.backup_job = job(false, Vec::new(), None, None)),
        ("101-backup-job-light", Screen::BackupJob, false, &|app| {
            app.state.backup_job = job(false, plan(), Some(BackupAt { item: 9, done: 1_820_000_000, of: 4_312_031_232, checking: false }), None)
        }),
        ("102-backup-job-dark-checking", Screen::BackupJob, true, &|app| {
            app.state.backup_job = job(false, plan(), Some(BackupAt { item: 11, done: 300_000_000, of: 1_420_455_936, checking: true }), None)
        }),
        ("103-backup-job-light-saved", Screen::BackupJob, false, &|app| {
            app.state.backup_job = job(false, plan(), None, Some(BackupEnd::Saved("20261021-200110".into())))
        }),
        ("104-backup-job-light-room", Screen::BackupJob, false, &|app| {
            let room = BackupRefusal::Room { needed: 52_400_000_000, free: 31_200_000_000 };
            app.state.backup_job = job(false, Vec::new(), None, Some(BackupEnd::Refused(room)))
        }),
        ("105-restore-job-light-checking", Screen::BackupJob, false, &|app| {
            app.state.backup_job = job(true, plan(), Some(BackupAt { item: 3, done: 12_000_000, of: 25_165_824, checking: true }), None)
        }),
        ("106-restore-job-dark-writing", Screen::BackupJob, true, &|app| {
            app.state.backup_job = job(true, plan(), Some(BackupAt { item: 10, done: 2_600_000_000, of: 4_202_635_264, checking: false }), None)
        }),
        ("107-restore-job-light-done", Screen::BackupJob, false, &|app| {
            let unmade = vec!["/data/user/10".into(), "/data/user_de/10".into(), "/data/misc_ce/10".into()];
            app.state.backup_job = job(true, plan(), None, Some(BackupEnd::Restored { unmade }))
        }),
        ("108-restore-job-light-failed", Screen::BackupJob, false, &|app| {
            let at = Some(BackupAt { item: 8, done: 400_000_000, of: 1_405_968_384, checking: false });
            app.state.backup_job = job(true, plan(), at, Some(BackupEnd::Failed("write /dev/block/dm-61: Input/output error (os error 5)".into())))
        }),
        ("109-restore-job-dark-refused", Screen::BackupJob, true, &|app| {
            app.state.backup_job = job(true, Vec::new(), None, Some(BackupEnd::Refused(BackupRefusal::DataReset)))
        }),
        ("49-advanced-light", Screen::Advanced, false, &|_| {}),
        ("50-files-light", Screen::Files, false, &|app| app.set_files("/".into(), Ok(sample_root()), LISTED)),
        ("51-files-dark", Screen::Files, true, &|app| app.set_files("/".into(), Ok(sample_root()), LISTED)),
        ("52-file-info-light", Screen::FileInfo, false, &|app| app.state.file = sample_root().into_iter().find(|e| e.name == "init.environ.rc")),
        ("61-files-names-light", Screen::Files, false, &|app| app.set_files("/sdcard/Download".into(), Ok(sample_download()), LISTED)),
        ("110-files-types-light", Screen::Files, false, &|app| app.set_files("/sdcard/Download".into(), Ok(sample_types()), LISTED)),
        ("110-files-types-dark", Screen::Files, true, &|app| app.set_files("/sdcard/Download".into(), Ok(sample_types()), LISTED)),
        ("62-files-empty-light", Screen::Files, false, &|app| app.set_files("/cust".into(), Ok(Vec::new()), LISTED)),
        ("63-files-unreadable-dark", Screen::Files, true, &|app| {
            app.set_files("/sys/kernel/debug".into(), Err("read /sys/kernel/debug: Permission denied (os error 13)".into()), LISTED)
        }),
        ("53-terminal-light", Screen::Terminal, false, &|app| {
            let (rows, cols) = app.term_size(&fonts);
            app.term_started(rows, cols);
            app.term_output(SHELL_SAMPLE);
        }),
        ("54-terminal-dark-symbols", Screen::Terminal, true, &|app| {
            let (rows, cols) = app.term_size(&fonts);
            app.term_started(rows, cols);
            app.term_output(SHELL_SAMPLE);
            app.state.layer = Layer::Symbols;
        }),
        ("75-terminal-light-ended", Screen::Terminal, false, &|app| {
            let (rows, cols) = app.term_size(&fonts);
            app.term_started(rows, cols);
            app.term_output(b"ishtar:/ # exit\r\n");
            app.term_ended("shell 已退出，退出码 0");
        }),
        ("55-language-light", Screen::Language, false, &|_| {}),
        ("56-time-zone-light", Screen::TimeZone, false, &|_| {}),
        ("56-time-zone-dark-london", Screen::TimeZone, true, &|app| app.state.time_zone = 5),
        ("57-logs-light", Screen::Logs, false, &|_| {}),
        ("58-license-light", Screen::License, false, &|_| {}),
        ("59-font-license-light", Screen::FontLicense, false, &|_| {}),
        ("60-slot-dark", Screen::Slot, true, &|app| {
            app.set_boot_slots(Ok(BootSlots { active: Slot::B, bootable: [false, true], successful: [true, true] }));
            app.state.next_slot = Some(Slot::A);
        }),
        ("60-slot-light-unread", Screen::Slot, false, &|app| {
            app.set_boot_slots(Err("android.hardware.boot@1.2::IBootControl is not running; the phone's boot control has to be started first".into()))
        }),
        ("70-erase-light", Screen::Erase, false, &|app| {
            app.set_asked(reset_asked());
            app.erase_started(true, true, vec![Part::Data, Part::Metadata]);
            app.erase_at(Part::Metadata);
        }),
        ("71-erase-dark-failed", Screen::Erase, true, &|app| {
            app.set_asked(reset_asked());
            app.erase_started(true, true, vec![Part::Data, Part::Metadata]);
            app.erase_at(Part::Metadata);
            app.erase_ended(
                Err(EraseStop::Failed(
                    Part::Metadata,
                    "/system/bin/make_f2fs -g android -O project_quota,extra_attr /dev/block/by-name/metadata ended with exit status: 1: Error: Device size is not sufficient for F2FS volume".into(),
                )),
                0,
            );
        }),
        ("72-erase-light-no-cache", Screen::Erase, false, &|app| {
            app.erase_started(false, true, Vec::new());
            app.erase_ended(Err(EraseStop::NoCache), 0);
        }),
        ("73-cant-start-light", Screen::CantStart, false, &|app| {
            app.set_asked(Asked { reason: Some("RescueParty".into()), ..Asked::default() })
        }),
        ("74-unhandled-dark", Screen::Unhandled, true, &|app| {
            app.set_asked(Asked { unhandled: Some("--update_package=/data/ota_package/update.zip".into()), ..Asked::default() })
        }),
        ("76-files-picked-light", Screen::Files, false, &|app| {
            app.set_files("/sdcard/Download".into(), Ok(sample_download()), LISTED);
            app.state.clip = Some(Clip { op: FileOp::Move, path: "/sdcard/README.txt".into() });
        }),
        ("77-folder-info-dark", Screen::FileInfo, true, &|app| in_download(app, "安装包")),
        ("78-file-info-light-deleting", Screen::FileInfo, false, &|app| {
            in_download(app, "README.txt");
            app.state.asking_delete = true;
        }),
        ("79-rename-light", Screen::Rename, false, &|app| {
            in_download(app, "README.txt");
            app.state.new_name = "README-old.txt".into();
        }),
        ("80-clash-dark", Screen::Clash, true, &|app| {
            app.file_clash(FileOp::Copy, "/sdcard/README.txt".into(), "/sdcard/Download".into(), vec!["README.txt".into()])
        }),
        ("81-file-job-light", Screen::Files, false, &|app| {
            app.file_job_started(copying());
            app.file_progress((2_934_000_000, 7_014_220_000), (0, 1));
        }),
        ("82-file-job-dark-measuring", Screen::Files, true, &|app| app.file_job_started(FileTask::Delete { path: "/sdcard/DCIM".into() })),
        ("83-file-job-light-stopped", Screen::Files, false, &|app| {
            app.file_job_started(copying());
            app.file_progress((2_934_000_000, 7_014_220_000), (0, 1));
            app.file_job_ended(JobEnd::Stopped, 0);
        }),
        ("84-file-job-dark-failed", Screen::Files, true, &|app| {
            app.file_job_started(FileTask::Move { path: "/tmp/skyward.log".into(), into: "/sdcard/Download".into() });
            app.file_progress((0, 18_000), (0, 1));
            app.file_job_ended(JobEnd::Failed("make /sdcard/Download/skyward.log: Read-only file system (os error 30)".into()), 0);
        }),
        ("85-decrypt-light-opening", Screen::Decrypt, false, &|app| app.state.data = Data::Opening),
        ("86-decrypt-light-digits", Screen::Decrypt, false, &|app| {
            app.data_waiting(Some((0, Keys::Digits)));
            app.state.code = "123".into();
        }),
        ("87-decrypt-dark-letters", Screen::Decrypt, true, &|app| app.data_waiting(Some((10, Keys::Letters)))),
        ("88-decrypt-light-checking", Screen::Decrypt, false, &|app| {
            app.state.data = Data::Locked { user: 0, keys: Keys::Digits, checking: true };
        }),
        ("89-decrypt-light-open", Screen::Decrypt, false, &|app| app.data_waiting(None)),
        ("90-decrypt-dark-failed", Screen::Decrypt, true, &|app| {
            app.data_failed("android.hardware.security.keymint.IKeyMintDevice/default is not running".into())
        }),
        ("91-wipe-light-all", Screen::Wipe, false, &|app| app.state.wipe[0] = true),
        ("92-wipe-dark-parts-cache", Screen::Wipe, true, &|app| {
            app.state.cache_partition = true;
            app.state.phone_logs = 7;
            app.state.wipe = [false, false, true, true, true, false, true, false, true];
        }),
        ("93-erase-light-media", Screen::Erase, false, &|app| {
            app.erase_started(false, false, vec![Part::Media, Part::AppCaches, Part::SystemCaches, Part::Logs]);
            app.erase_at(Part::AppCaches);
            app.erase_progress(41_380, 128_906);
        }),
        ("94-erase-light-reset-done", Screen::Erase, false, &|app| {
            app.erase_started(true, false, vec![Part::Data, Part::Metadata]);
            app.erase_ended(Ok(()), 0);
        }),
        ("95-erase-dark-merging", Screen::Erase, true, &|app| {
            app.erase_started(true, false, Vec::new());
            app.erase_ended(Err(EraseStop::Update(Hold::Merging)), 0);
        }),
        ("97-decrypt-light-erased", Screen::Decrypt, false, &|app| app.state.data = Data::Erased),
        ("98-erase-light-closed", Screen::Erase, false, &|app| {
            app.erase_started(false, false, vec![Part::Art]);
            app.erase_ended(Err(EraseStop::Closed("android.hardware.security.keymint.IKeyMintDevice/default is not running".into())), 0);
        }),
        ("99-erase-dark-locked", Screen::Erase, true, &|app| {
            app.erase_started(false, false, vec![Part::AppData, Part::AppCaches]);
            app.erase_ended(Err(EraseStop::Locked(vec![0, 999])), 0);
        }),
    ];
    for (name, screen, dark, set) in pages {
        let mut app = opened(screen, w, h, ppd);
        app.state.dark = dark;
        set(&mut app);
        let path = out.join(format!("{name}.png"));
        write_png(&path, &whole_page(&mut app, &fonts, &info))?;
        println!("wrote {}", path.display());
    }

    // The panel mid-gesture, to set beside recordings of the same moves.
    let moments: [Moment; 18] = [
        ("08-about-light", Screen::About, false, &|_, _| 2000),
        ("08-about-dark", Screen::About, true, &|_, _| 2000),
        ("08-about-light-later", Screen::About, false, &|_, _| 9000),
        ("21-settings-scrolled", Screen::Settings, false, &|app, f| drag(app, f, 1900.0, 1300.0)),
        ("22-settings-scrolled-dark", Screen::Settings, true, &|app, f| drag(app, f, 1900.0, 1300.0)),
        ("23-specs-title-fading", Screen::Specs, false, &|app, f| drag(app, f, 1900.0, 1870.0)),
        ("24-specs-title-gone", Screen::Specs, false, &|app, f| drag(app, f, 1900.0, 1820.0)),
        ("25-specs-under-bar", Screen::Specs, false, &|app, f| drag(app, f, 1900.0, 1450.0)),
        ("26-reboot-pulled-down", Screen::Reboot, false, &|app, f| hold(app, f, 900.0, 1700.0)),
        ("27-back-cue-short", Screen::Home, false, &|app, f| swipe_in(app, f, &info, 133.0)),
        ("28-back-cue-armed", Screen::Home, false, &|app, f| swipe_in(app, f, &info, 323.0)),
        ("29-about-scrolled", Screen::About, false, &|app, f| drag(app, f, 1900.0, 1500.0)),
        ("64-files-scrolled", Screen::Files, false, &|app, f| {
            app.set_files("/system/bin".into(), Ok(sample_bin()), LISTED);
            app.frame(f, &info);
            drag(app, f, 2000.0, 700.0)
        }),
        ("65-files-scrolled-dark", Screen::Files, true, &|app, f| {
            app.set_files("/system/bin".into(), Ok(sample_bin()), LISTED);
            app.frame(f, &info);
            drag(app, f, 2000.0, 700.0)
        }),
        // The fastboot page starts worked with the keys, its first choice lit.
        ("68-fastboot-light", Screen::Fastboot, false, &|app, _| {
            app.set_fastboot(true, UsbLink::Connected);
            0
        }),
        ("69-fastboot-dark-unplugged", Screen::Fastboot, true, &|app, f| {
            app.set_fastboot(true, UsbLink::Unplugged);
            keys_down(app, f, &info, 1)
        }),
        ("111-home-keys-light", Screen::Home, false, &|app, f| keys_down(app, f, &info, 3)),
        ("112-home-keys-dark", Screen::Home, true, &|app, f| keys_down(app, f, &info, 9)),
    ];
    for (name, screen, dark, act) in moments {
        let mut app = opened(screen, w, h, ppd);
        app.state.dark = dark;
        app.frame(&fonts, &info);
        app.tick(0);
        let now = act(&mut app, &fonts);
        // Time passes frame by frame, as on the panel.
        for ms in (0..now).step_by(16) {
            app.tick(ms);
        }
        app.tick(now);
        let mut c = Canvas::new(w, h);
        app.compose(&mut c, &fonts, &info, CLOCK, now);
        let path = out.join(format!("{name}.png"));
        write_png(&path, &c)?;
        println!("wrote {}", path.display());
    }

    // The status bar's battery in each state, light and dark, one bar a row.
    for (name, theme) in [("66-battery-light", light), ("67-battery-dark", Theme::dark(w, h, ppd))] {
        let path = out.join(format!("{name}.png"));
        write_png(&path, &battery_states(&fonts, &theme))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Status bars over the page colour, one for each state the battery shows:
/// charging, plain levels with the cable out, low ones, full.
fn battery_states(f: &Fonts, t: &Theme) -> Canvas {
    let states: [(u8, bool); 8] = [(50, true), (15, true), (100, true), (100, false), (50, false), (30, false), (15, false), (5, false)];
    let row = t.m.dp(52.0).round() as u32;
    let mut sheet = Canvas::new(t.m.w, row * states.len() as u32);
    for (k, &battery) in states.iter().enumerate() {
        let mut bar = Canvas::new(t.m.w, row);
        bar.clear(t.pal.bg);
        widgets::status_bar(&mut bar, t, f, &Bar { clock: CLOCK, battery: Some(battery) }, t.pal.status);
        sheet.put(&bar, (row as usize * k) as i32);
    }
    sheet
}

/// File name, page, dark theme, and the touches leading up to the moment,
/// which return the time it is shown at.
type Moment<'a> = (&'a str, Screen, bool, &'a dyn Fn(&mut App, &Fonts) -> u64);

/// Drag from `y0` to `y1` at x = 600 and rest before lifting, so the page
/// stays where the finger left it; returns a time after the bar has
/// settled.
fn drag(app: &mut App, f: &Fonts, y0: f32, y1: f32) -> u64 {
    app.touch(Touch::Down(600.0, y0), f, 0);
    for k in 1..=10u64 {
        app.touch(Touch::Move(600.0 + (k % 2) as f32, y0 + (y1 - y0) * k as f32 / 10.0), f, 40 * k);
    }
    app.touch(Touch::Up, f, 800);
    for ms in (800..2000).step_by(8) {
        app.tick(ms);
    }
    2000
}

/// Press the volume key that moves the light down the page `times` times,
/// a second apart, on the page drawn; returns a time after the page has
/// come to rest.
fn keys_down(app: &mut App, f: &Fonts, info: &DeviceInfo, times: u64) -> u64 {
    for k in 0..times {
        app.frame(f, info);
        app.key(Key::VolumeDown, k * 1000);
        app.key_up(Key::VolumeDown, k * 1000 + 100);
        for ms in (k * 1000..(k + 1) * 1000).step_by(16) {
            app.tick(ms);
        }
    }
    times * 1000
}

/// Pull from `y0` to `y1` at x = 600 and keep the finger down.
fn hold(app: &mut App, f: &Fonts, y0: f32, y1: f32) -> u64 {
    app.touch(Touch::Down(600.0, y0), f, 0);
    for k in 1..=10u64 {
        app.touch(Touch::Move(600.0, y0 + (y1 - y0) * k as f32 / 10.0), f, 40 * k);
    }
    400
}

/// Open the reboot page from the home page, which leaves a page to go back
/// to, then swipe in from the left edge at y = 1300 to `x` and keep the
/// finger down, the way the 17U recordings were made.
fn swipe_in(app: &mut App, f: &Fonts, info: &DeviceInfo, x: f32) -> u64 {
    let hits = &app.frame(f, info).hits;
    let (rx, ry) = hits.iter().find(|h| h.action == Action::Open(Screen::Reboot)).expect("the home page opens the reboot page").rect.centre();
    app.touch(Touch::Down(rx, ry), f, 0);
    app.touch(Touch::Up, f, 0);
    app.touch(Touch::Down(3.0, 1300.0), f, 0);
    for k in 1..=8u64 {
        app.touch(Touch::Move(3.0 + (x - 3.0) * k as f32 / 8.0, 1300.0 + k as f32), f, 8 * k);
    }
    64
}

/// The slot's partitions on a Xiaomi 13 Ultra, in the order super lists
/// them and where the recovery mounts them, all mounted but `unmounted`.
/// The touch programs run from odm.
fn sample_mounts(unmounted: &[&str]) -> Vec<MountRow> {
    let partitions = [
        ("odm", "/odm"),
        ("product", "/mnt/product"),
        ("system", "/mnt/system"),
        ("system_dlkm", "/system_dlkm"),
        ("system_ext", "/mnt/system_ext"),
        ("vendor", "/vendor"),
        ("vendor_dlkm", "/vendor_dlkm"),
        ("mi_ext", "/mnt/mi_ext"),
    ];
    partitions
        .iter()
        .map(|&(base, point)| {
            let mounted = !unmounted.contains(&base);
            let holders = if mounted && base == "odm" { vec!["touch_report".into(), "touchsensor".into()] } else { Vec::new() };
            MountRow { base: base.into(), point: point.into(), mounted, holders }
        })
        .collect()
}

/// When the previews' folders are listed: 2026-10-19 23:46 in Beijing, the
/// time their status bar shows.
const LISTED: i64 = 1_792_424_760;

/// The ramdisk's timestamp on every file a recovery image carries.
const RAMDISK_TIME: i64 = 1_230_768_000;

fn link(name: &str, target: &str, folder: bool) -> FileRow {
    let kind = FileKind::Link { target: target.into(), folder };
    FileRow { name: name.into(), kind, size: target.len() as u64, modified: RAMDISK_TIME, mode: 0o120_777, uid: 0, gid: 0 }
}

fn folder(name: &str, items: usize, modified: i64) -> FileRow {
    FileRow { name: name.into(), kind: FileKind::Folder { items: Some(items) }, size: 4096, modified, mode: 0o040_755, uid: 0, gid: 0 }
}

fn file(name: &str, size: u64, modified: i64) -> FileRow {
    FileRow { name: name.into(), kind: FileKind::File, size, modified, mode: 0o100_644, uid: 0, gid: 0 }
}

/// Part of the root folder of a Xiaomi 13 Ultra, as `ls -la /` lists it,
/// in the file manager's order.
fn sample_root() -> Vec<FileRow> {
    let made = RAMDISK_TIME;
    vec![
        folder("apex", 31, made),
        link("bin", "/system/bin", true),
        link("cache", "/data/cache", true),
        folder("data", 0, made),
        folder("dev", 142, LISTED - 3600),
        link("etc", "/system/etc", true),
        folder("metadata", 9, made),
        folder("mnt", 14, LISTED - 3600),
        folder("odm", 7, made),
        folder("proc", 612, LISTED - 3600),
        folder("product", 0, made),
        link("sdcard", "/storage/self/primary", true),
        folder("system", 9, made),
        folder("vendor", 11, made),
        link("init", "/system/bin/init", false),
        FileRow { name: "init.environ.rc".into(), kind: FileKind::File, size: 621, modified: made, mode: 0o100_750, uid: 0, gid: 2000 },
    ]
}

/// A download folder whose names take one line, two, or more than two.
fn sample_download() -> Vec<FileRow> {
    let (today, this_year, earlier) = (LISTED - 4 * 3600, 1_790_380_800, 1_667_692_800);
    vec![
        folder("Skyward", 3, today),
        folder("安装包", 68, this_year),
        file("1d9bf122-130e-4db7-ae55-460c84819f0e.pdf", 1_090_000, this_year),
        file("2021一带一路暨金砖大赛之物联网技术及其在智慧城市与工业互联网领域的应用创新赛表彰文件.pdf", 634_960, earlier),
        file("OTA_ishtar_OS3.0.305.0.WMACNXM_20260412_full_package_signed_for_recovery_install.zip", 7_014_220_000, this_year),
        file("README.txt", 900, today),
        file("skyward-recovery-ishtar.img", 104_857_600, today),
    ]
}

/// A folder holding a file of each type the list draws an icon of its own
/// for, and one it does not know, in the file manager's order.
fn sample_types() -> Vec<FileRow> {
    let made = LISTED - 2 * 3600;
    let mut rows: Vec<FileRow> = [
        ("boot.img", 100_663_296),
        ("IMG_20261021_143005.jpg", 4_812_000),
        ("update-ishtar.zip", 1_402_000_000),
        ("app-release.apk", 11_300_000),
        ("recovery.log", 486_000),
        ("VID_20261020_201512.mp4", 182_400_000),
        ("会议录音.m4a", 8_900_000),
        ("build.prop", 3_200),
        ("年度报告.docx", 268_000),
        ("预算.xlsx", 54_000),
        ("方案汇报.pptx", 6_100_000),
        ("用户手册.pdf", 2_300_000),
        ("persist.bak", 33_554_432),
    ]
    .into_iter()
    .map(|(name, size)| file(name, size, made))
    .collect();
    rows.sort_by_key(|r| r.name.to_lowercase());
    rows
}

/// A Xiaomi 13 Ultra's /system/bin: programs, and the links to toybox that
/// stand for its commands, in the file manager's order.
fn sample_bin() -> Vec<FileRow> {
    let programs = [
        "abb", "am", "app_process32", "app_process64", "applypatch", "appops", "appwidget", "atrace", "audioserver", "auditctl",
        "blkid", "bmgr", "bootanimation", "bootstat", "bpfloader", "bugreport", "bugreportz", "cameraserver", "cmd", "content",
        "crash_dump32", "crash_dump64", "debuggerd", "device_config", "dmctl", "dpm", "dumpstate", "dumpsys", "e2fsck", "e2fsdroid",
        "fsck.f2fs", "fsck_msdos", "gatekeeperd", "gpuservice", "hid", "idmap2", "ime", "incident", "incidentd", "init", "input",
        "installd", "ip", "iptables", "keystore2", "lmkd", "locksettings", "logcat", "logd", "lpdump", "lshal", "make_f2fs",
        "mediaserver", "mke2fs", "monkey", "netd", "odsign", "perfetto", "ping", "pm", "reboot", "resize2fs", "run-as",
        "screencap", "screenrecord", "sdcard", "sensorservice", "service", "servicemanager", "settings", "sgdisk", "sh",
        "simpleperf", "sm", "snapshotctl", "storaged", "surfaceflinger", "svc", "tombstoned", "toolbox", "toybox", "traced",
        "tune2fs", "uiautomator", "uncrypt", "update_engine", "update_verifier", "vdc", "vold", "wificond", "wm",
    ];
    let commands = [
        "base64", "basename", "cat", "chcon", "chmod", "chown", "cmp", "cp", "cut", "date", "dd", "df", "diff", "dirname", "dmesg",
        "du", "echo", "env", "expr", "find", "free", "getprop", "grep", "head", "hostname", "id", "ifconfig", "insmod", "kill",
        "ln", "ls", "lsmod", "lsof", "md5sum", "mkdir", "mount", "mv", "netstat", "nohup", "pgrep", "pidof", "pkill", "printf",
        "ps", "pwd", "readlink", "rm", "rmdir", "sed", "sendevent", "seq", "setprop", "sleep", "sort", "start", "stat", "stop",
        "tail", "tar", "tee", "top", "touch", "tr", "umount", "uname", "uptime", "wc", "which", "xargs", "xxd",
    ];
    let mut rows = vec![folder("hw", 34, RAMDISK_TIME)];
    let mut rest: Vec<FileRow> = programs
        .iter()
        .enumerate()
        .map(|(k, name)| FileRow { mode: 0o100_755, gid: 2000, ..file(name, 18_000 + 7_919 * k as u64 % 900_000, RAMDISK_TIME) })
        .chain(commands.iter().map(|name| link(name, "toybox", false)))
        .collect();
    rest.sort_by_key(|r| r.name.to_lowercase());
    rows.extend(rest);
    rows
}

/// A page as most previews show it: on a phone whose data partition is
/// open, its owner's own files with it.
fn opened(screen: Screen, w: u32, h: u32, ppd: f32) -> App {
    let mut app = App::new(screen, w, h, ppd);
    app.state.data = Data::Open;
    app
}

/// The full length of a page with the status bar over its top, the way a
/// long screenshot shows it: a long list's rows and a pinned strip in the
/// places an unscrolled page gives them.
fn whole_page(app: &mut App, f: &Fonts, info: &DeviceInfo) -> Canvas {
    let t = app.theme();
    let (screen, state) = (app.screen(), app.state.clone());
    let frame = app.frame(f, info);
    let mut c = Canvas::new(frame.canvas.w, frame.height(&t) as u32);
    c.clear(frame.bg);
    c.put(&frame.canvas, 0);
    if let Some(rows) = &frame.rows {
        let cx = Ctx { t: &t, f, info, state: &state, term: None };
        for k in 0..rows.count() {
            let mut row = Canvas::new(c.w, (rows.edges[k + 1] - rows.edges[k]) as u32);
            pages::row(screen, &cx, k, &mut row);
            c.put(&row, rows.edges[k] as i32);
        }
    }
    if let Some(p) = &frame.pinned {
        c.put(&p.canvas, p.top.round() as i32);
    }
    if let Some(d) = &frame.dock {
        c.put(&d.canvas, (c.h - d.canvas.h) as i32);
    }
    let battery = info.battery.as_ref().map(|b| (b.level, b.plugged()));
    widgets::status_bar(&mut c, &t, f, &Bar { clock: CLOCK, battery }, t.pal.status);
    c
}

/// Xiaomi 17 Ultra by Leica as `skyward info` read it on 2026-10-18 (the
/// config supplies the SoC name, panel and screen size).
fn nezha(cfg: &DeviceConfig) -> DeviceInfo {
    DeviceInfo {
        model: "Xiaomi 17 Ultra by Leica".into(),
        codename: cfg.device.codename.clone(),
        soc: cfg.device.soc.clone(),
        cpu: Cpu { cores: 8, max_khz: Some(4_608_000) },
        ram_kib: 15_472_464,
        panel: Some((cfg.display.width, cfg.display.height)),
        screen: cfg.display.diagonal,
        os_version: Some("OS4.0.4.0.XPACNXM".into()),
        android: Some("17".into()),
        security_patch: Some(PatchLevel { year: 2026, month: 10, day: 1 }),
        kernel: "6.12.69-android16-6-g43b5ab7e247d-abogki561813157-4k".into(),
        storage: Storage { total: 1_039_535_157_248, free: Some(620_652_802_048) },
        battery: Some(Battery {
            level: 95,
            charge: Charge::Charging,
            temp: Some(350),
            health: Some("Good".into()),
            cycles: Some(300),
            design: Some(6_800_000),
            full: Some(6_543_000),
            voltage: Some(4_442_000),
        }),
        slot: Some(Slot::B),
        snapshot: SnapshotState::None,
        boot: Boot { unlocked: Some(true), verified: Some("orange".into()) },
    }
}

/// Weight ladders for matching stroke widths against device screenshots: the
/// settings strings at their sizes, and a capital H at the wordmark size.
fn calibrate(fonts: &Fonts, t: &Theme) -> Result<Canvas> {
    let m = &t.m;
    let weights = [330.0, 380.0, 420.0, 450.0, 480.0, 520.0, 560.0, 600.0, 620.0, 650.0, 700.0];
    let mut c = Canvas::new(m.w, 200 * weights.len() as u32);
    c.clear(t.pal.bg);
    let ink = t.pal.text;
    for (i, w) in weights.iter().enumerate() {
        let face = fonts.face(*w)?;
        let top = 10.0 + 200.0 * i as f32;
        c.text_top(&face, "设置", 10.0, top, m.title_px, ink, None);
        c.text_top(&face, "蓝牙", 210.0, top, m.row_title_px, ink, None);
        c.text_top(&face, "亮度", 340.0, top, m.sub_px, ink, None);
        c.text_top(&face, "登录小米账号", 440.0, top, m.account_title_px, ink, None);
        let b = top + 110.0;
        c.text_top(&face, "享受更多小米服务", 10.0, b, m.sub_px, ink, None);
        c.text_top(&face, "已开启", 340.0, b, m.sub_px, ink, None);
        c.text_top(&face, "搜索系统设置项", 480.0, b, m.row_title_px, ink, None);
        c.text(&face, "15:29", 840.0, b + m.status_px * 0.72, m.status_px, ink, None);
        c.text_top(&face, "新版本", 1010.0, b, m.badge_px, ink, None);
        // The stock wordmark is set at a 136.5 px em on the 3.5 px/dp
        // reference panel.
        c.text(&face, "H", 1100.0, top + 105.0, 136.5, ink, None);
    }
    Ok(c)
}

fn font_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("SKYWARD_FONT") {
        return Ok(PathBuf::from(p));
    }
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/fonts/MiSansVF.ttf");
    if bundled.exists() {
        return Ok(bundled);
    }
    for p in ["/System/Library/Fonts/STHeiti Medium.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc"] {
        if Path::new(p).exists() {
            return Ok(PathBuf::from(p));
        }
    }
    Err(anyhow!("no font found; set SKYWARD_FONT to a .ttf/.ttc path"))
}

fn write_png(path: &Path, canvas: &Canvas) -> Result<()> {
    let file = File::create(path)?;
    let w = BufWriter::new(file);
    let mut enc = png::Encoder::new(w, canvas.w, canvas.h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&canvas.data)?;
    Ok(())
}
