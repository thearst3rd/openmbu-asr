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
const FLAGS_OFFSET: u32 = 20;

const FLAG_IS_LOADING: u32 = 1 << 0;
const FLAG_LEVEL_STARTED: u32 = 1 << 1;
const FLAG_LEVEL_FINISHED: u32 = 1 << 2;
const FLAG_EGG_FOUND: u32 = 1 << 3;
const FLAG_QUIT_TO_MENU: u32 = 1 << 4;

asr::async_main!(stable);
asr::panic_handler!();

#[derive(Gui)]
struct Settings {
    /// Split on easter egg collection
    ///
    /// If checked, split when collecting an easter egg.
    #[default = false]
    split_on_egg: bool,

    /// Only start timer on first level
    ///
    /// If checked, the timer will not start unless you play the first level in a difficulty. This makes practicing
    /// later levels easier without messing up your splits.
    #[default = true]
    only_start_on_first: bool,

    /// Auto reset on quitting (CAUTION!!)
    ///
    /// If checked, the run will automatically reset when quitting back to the menu if the last level you finished is
    /// any level except the last level in a difficulty
    #[default = false]
    auto_reset: bool,
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
                // Watcher for each flag
                let mut is_loading_watcher: Watcher<bool> = Watcher::new();
                let mut level_started_watcher: Watcher<bool> = Watcher::new();
                let mut level_finished_watcher: Watcher<bool> = Watcher::new();
                let mut egg_found_watcher: Watcher<bool> = Watcher::new();
                let mut quit_to_menu_watcher: Watcher<bool> = Watcher::new();

                let mut last_level_finished: i32 = -1;

                // Since we found the pointer, these should™ read successfully
                if let Ok(level) = process.read(data_address + LEVEL_OFFSET) {
                    print_limited::<128>(&format_args!("Initial value for current level: {}", level));
                    level_watcher.update_infallible(level);
                } else {
                    print_message("Failed to read current level!!");
                    level_watcher.update_infallible(-1);
                }

                if let Ok(flags) = process.read::<u32>(data_address + FLAGS_OFFSET) {
                    print_limited::<128>(&format_args!("Initial value for flags: {}", flags));
                    is_loading_watcher.update_infallible((flags & FLAG_IS_LOADING) != 0);
                    level_started_watcher.update_infallible((flags & FLAG_LEVEL_STARTED) != 0);
                    level_finished_watcher.update_infallible((flags & FLAG_LEVEL_FINISHED) != 0);
                    egg_found_watcher.update_infallible((flags & FLAG_EGG_FOUND) != 0);
                    quit_to_menu_watcher.update_infallible((flags & FLAG_QUIT_TO_MENU) != 0);
                } else {
                    print_message("Failed to read is loading!!");
                    is_loading_watcher.update_infallible(false);
                    level_started_watcher.update_infallible(false);
                    level_finished_watcher.update_infallible(false);
                    egg_found_watcher.update_infallible(false);
                    quit_to_menu_watcher.update_infallible(false);
                }

                loop {
                    settings.update();

                    level_watcher.update(process.read(data_address + LEVEL_OFFSET).ok());
                    if let Ok(flags) = process.read::<u32>(data_address + FLAGS_OFFSET) {
                        is_loading_watcher.update_infallible((flags & FLAG_IS_LOADING) != 0);
                        level_started_watcher.update_infallible((flags & FLAG_LEVEL_STARTED) != 0);
                        level_finished_watcher.update_infallible((flags & FLAG_LEVEL_FINISHED) != 0);
                        egg_found_watcher.update_infallible((flags & FLAG_EGG_FOUND) != 0);
                        quit_to_menu_watcher.update_infallible((flags & FLAG_QUIT_TO_MENU) != 0);
                    } else {
                        print_message("Failed to read flags!");
                    }

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
                                    let mut should_start: bool = timer::state() == timer::TimerState::NotRunning;
                                    if should_start && settings.only_start_on_first {
                                        should_start = level.current == 1 || level.current == 21 || level.current == 41;
                                    }
                                    if should_start {
                                        timer::start();
                                        timer::pause_game_time();
                                        last_level_finished = -1;
                                        print_limited::<128>(&format_args!("Last level finished: {}", last_level_finished));
                                    }
                                }
                            }
                        }
                        if let Some(level_finished) = level_finished_watcher.pair {
                            if level_finished.changed() {
                                print_limited::<128>(&format_args!("Level finished changed: {} -> {}", level_finished.old, level_finished.current));
                                if level_finished.current {
                                    timer::split();
                                    last_level_finished = level.current;
                                    print_limited::<128>(&format_args!("Last level finished: {}", last_level_finished));
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
                        if let Some(quit_to_menu) = quit_to_menu_watcher.pair {
                            if quit_to_menu.changed() {
                                print_limited::<128>(&format_args!("Quit to menu changed: {} -> {}", quit_to_menu.old, quit_to_menu.current));
                                if quit_to_menu.current && settings.auto_reset {
                                    let not_last_level = last_level_finished != 20 && last_level_finished != 40 && last_level_finished != 60;
                                    let got_easter_egg = settings.split_on_egg && egg_found_watcher.pair.unwrap().current;
                                    if not_last_level && !got_easter_egg {
                                        timer::reset();
                                    }
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
