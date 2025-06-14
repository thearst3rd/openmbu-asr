#![no_std]

use asr::{future::{next_tick, retry}, print_limited, print_message, settings::Gui, signature::Signature, timer, watcher::Watcher, Address, Process};

static EXECUTABLE_NAMES: [&str; 6] = [
    "MBUltra.exe",
    "MBUltra64.exe",
    "MBUltra_DEBUG.exe",
    "MBUltra64_DEBUG.exe",
    "MBUltra_OPTIMIZEDDEBUG.exe",
    "MBUltra64_OPTIMIZEDDEBUG.exe",
];

static SIG: Signature<15> = Signature::new("4F 4D 42 55 5F 41 53 52 5F 61 62 63 64 65 66"); // "OMBU_ASR_abcdef"

const LEVEL_OFFSET: u32 = 16;
const IS_LOADING_OFFSET: u32 = 20;
const LEVEL_STARTED_OFFSET: u32 = 21;
const LEVEL_FINISHED_OFFSET: u32 = 22;
const EGG_FOUND_OFFSET: u32 = 23;

asr::async_main!(stable);
asr::panic_handler!();

#[derive(Gui)]
struct Settings {
    /// Split on Easter Egg Collection
    #[default = true]
    split_on_egg: bool,
}

async fn main() {
    // TODO: Set up some general state and settings.
    let mut settings = Settings::register();

    loop {
        let process = retry(|| {
            EXECUTABLE_NAMES.into_iter().find_map(Process::attach)
        }).await;
        process
            .until_closes(async {
                let mut data_address_option: Option<Address> = None;

                print_message("Scanning for autosplitter data");
                while data_address_option.is_none() {
                    for range in process.memory_ranges() {
                        if let (Ok(address), Ok(len)) = (range.address(), range.size()) {
                            data_address_option = SIG.scan_process_range(&process, (address, len));
                            if data_address_option.is_some() {
                                print_limited::<128>(&format_args!("Found autosplitter data at {}", data_address_option.unwrap()));
                                break;
                            }
                        }
                    }
                }

                let data_address = data_address_option.unwrap();

                let mut level_watcher: Watcher<i32> = Watcher::new();
                let mut is_loading_watcher: Watcher<bool> = Watcher::new();
                let mut level_started_watcher: Watcher<bool> = Watcher::new();
                let mut level_finished_watcher: Watcher<bool> = Watcher::new();
                let mut egg_found_watcher: Watcher<bool> = Watcher::new();

                // Since we found the pointer, these should™ read successfully
                if let Ok(level) = process.read(data_address + LEVEL_OFFSET) {
                    print_limited::<128>(&format_args!("Initial value for current level: {}", level));
                    level_watcher.update_infallible(level);
                } else {
                    print_message("Failed to read current level!!");
                    level_watcher.update_infallible(-1);
                }

                if let Ok(is_loading) = process.read(data_address + IS_LOADING_OFFSET) {
                    print_limited::<128>(&format_args!("Initial value for is loading: {}", is_loading));
                    is_loading_watcher.update_infallible(is_loading);
                } else {
                    print_message("Failed to read is loading!!");
                    is_loading_watcher.update_infallible(false);
                }

                if let Ok(level_started) = process.read(data_address + LEVEL_STARTED_OFFSET) {
                    print_limited::<128>(&format_args!("Initial value for level started: {}", level_started));
                    level_started_watcher.update_infallible(level_started);
                } else {
                    print_message("Failed to read level started!!");
                    level_started_watcher.update_infallible(false);
                }

                if let Ok(level_finished) = process.read(data_address + LEVEL_FINISHED_OFFSET) {
                    print_limited::<128>(&format_args!("Initial value for level finished: {}", level_finished));
                    level_finished_watcher.update_infallible(level_finished);
                } else {
                    print_message("Failed to read level finished!!");
                    level_finished_watcher.update_infallible(false);
                }

                if let Ok(egg_found) = process.read(data_address + EGG_FOUND_OFFSET) {
                    print_limited::<128>(&format_args!("Initial value for egg found: {}", egg_found));
                    egg_found_watcher.update_infallible(egg_found);
                } else {
                    print_message("Failed to read egg found!!");
                    egg_found_watcher.update_infallible(false);
                }

                loop {
                    settings.update();

                    level_watcher.update(process.read(data_address + LEVEL_OFFSET).ok());
                    is_loading_watcher.update(process.read(data_address + IS_LOADING_OFFSET).ok());
                    level_started_watcher.update(process.read(data_address + LEVEL_STARTED_OFFSET).ok());
                    level_finished_watcher.update(process.read(data_address + LEVEL_FINISHED_OFFSET).ok());
                    egg_found_watcher.update(process.read(data_address + EGG_FOUND_OFFSET).ok());

                    if let Some(level) = level_watcher.pair {
                        if level.changed() {
                            print_limited::<128>(&format_args!("Changed level: {} -> {}", level.old, level.current));
                        }
                        // By this point, all of these should be read properly
                        if let Some(is_loading) = is_loading_watcher.pair {
                            if is_loading.changed() {
                                print_limited::<128>(&format_args!("Loading changed: {} -> {}", is_loading.old, is_loading.current));
                                if is_loading.current {
                                    timer::pause_game_time();
                                } else {
                                    timer::resume_game_time();
                                }
                            }
                        }
                        if let Some(level_started) = level_started_watcher.pair {
                            if level_started.changed() {
                                print_limited::<128>(&format_args!("Level started changed: {} -> {}", level_started.old, level_started.current));
                                if level_started.current {
                                    timer::start();
                                    timer::pause_game_time();
                                }
                            }
                        }
                        if let Some(level_finished) = level_finished_watcher.pair {
                            if level_finished.changed() {
                                print_limited::<128>(&format_args!("Level finished changed: {} -> {}", level_finished.old, level_finished.current));
                                if level_finished.current {
                                    timer::split();
                                }
                            }
                        }
                        if let Some(egg_found) = egg_found_watcher.pair {
                            if egg_found.changed() {
                                print_limited::<128>(&format_args!("Egg found changed: {} -> {}", egg_found.old, egg_found.current));
                                if egg_found.current && settings.split_on_egg {
                                    timer::split();
                                }
                            }
                        }
                    } else {
                        print_message("Failed to read current level!!");
                    }

                    next_tick().await;
                }
            })
            .await;
    }
}
