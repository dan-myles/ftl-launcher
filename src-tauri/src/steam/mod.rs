use anyhow::Result;
use fs_extra::dir::get_size;
use lazy_static::lazy_static;
use std::collections::VecDeque;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::RwLock;
use tokio::{task, time};

pub mod client;

// NOTE: We are using mainly RwLocks here because we dont need to be able to
// write to most of them, all of the time, We need to be able to read from them most of the time.
lazy_static! {
    static ref IS_CALLBACK_DAEMON_RUNNING: Arc<RwLock<bool>> = Arc::new(RwLock::new(false));
    static ref IS_MOD_DAEMON_RUNNING: Arc<RwLock<bool>> = Arc::new(RwLock::new(false));
    static ref MOD_DOWNLOAD_QUEUE: Arc<RwLock<VecDeque<u64>>> =
        Arc::new(RwLock::new(VecDeque::new()));
}

/**
* function: mdq_clear
* ---
* Clears the mod download queue.
*/
#[tauri::command]
pub async fn mdq_clear() -> Result<(), String> {
    let mod_queue_ref = MOD_DOWNLOAD_QUEUE.clone();
    let mut mod_queue = mod_queue_ref.write().await;
    (*mod_queue).clear();
    Ok(())
}

/**
* function: mdq_mod_add
* ---
* Adds a mod to the download queue.
*/
#[tauri::command]
pub async fn mdq_mod_add(published_file_id: u64) -> Result<(), String> {
    let mod_queue_ref = MOD_DOWNLOAD_QUEUE.clone();
    let mut mod_queue = mod_queue_ref.write().await;

    // Check if the mod is already in the queue
    // We do not return an error here as the frontend can call this rapidly
    // and we don't want the UI out of sync. (user spams a button)
    if (*mod_queue).contains(&published_file_id) {
        return Ok(());
    }

    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Now check if the mod is already installed
    let ugc = client.ugc();
    let is_installed = ugc.item_install_info(steamworks::PublishedFileId(published_file_id));

    match is_installed {
        Some(_) => {
            (*mod_queue).push_back(published_file_id);
            return Err("Mod already installed!".to_string());
        }
        None => {
            (*mod_queue).push_back(published_file_id);
            Ok(())
        }
    }
}

/**
* function: mdq_mod_remove
* ---
* Removes a mod from the download queue.
*/
#[tauri::command]
pub async fn mdq_mod_remove(published_file_id: u64) -> Result<(), String> {
    let mod_queue_ref = MOD_DOWNLOAD_QUEUE.clone();
    let mut mod_queue = mod_queue_ref.write().await;

    if (*mod_queue).contains(&published_file_id) {
        (*mod_queue).retain(|&x| x != published_file_id);
        Ok(())
    } else {
        Err("Mod not found in download queue!".to_string())
    }
}

/**
* function: mdq_active_download_progress
* ---
* Returns the progress of an active mod download. Will error if there is no active download.
*/
#[tauri::command]
pub async fn mdq_active_download_progress() -> Result<[u64; 2], String> {
    // Grab the mod queue and drop it like its hot!
    // Don't carry those locks across awaits 😎
    let mod_queue_ref = MOD_DOWNLOAD_QUEUE.clone();
    let mod_queue = mod_queue_ref.read().await;
    let front = (*mod_queue).front();
    let front = front.cloned();
    drop(mod_queue);

    if front.is_none() {
        return Err("No active download!".to_string());
    }

    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("Now steam client found!".to_string());
    }
    let client = client.unwrap();

    // Get the download progress
    let ugc = client.ugc();
    let download_progress = ugc
        .item_download_info(steamworks::PublishedFileId(front.unwrap()))
        .ok_or("There was an error getting your download progress!".to_string())?;

    // Return "front" download progress
    Ok([download_progress.0, download_progress.1])
}

/**
* function: mdq_active_download_id
* ---
* Returns the workshopId of an active mod download. Will error if there is no active download.
*/
#[tauri::command]
pub async fn mdq_active_download_id() -> Result<u64, String> {
    let mod_queue_ref = MOD_DOWNLOAD_QUEUE.clone();
    let mod_queue = mod_queue_ref.read().await;
    let front = (*mod_queue).front();

    match front {
        Some(id) => Ok(*id),
        None => Err("No active download!".to_string()),
    }
}

/**
* function: mdq_start_daemon
* ---
* Starts the mod download queue daemon. This daemon will check if there are any mods in the queue
* then download them. This daemon will run continuously until the app is closed. Handles,
* unmounting of the steam api, and checking if the mod is already installed.
* Emits a "mdq_active_download_info" event while a mod is downloading.
*/
#[tauri::command]
pub async fn mdq_start_daemon(app_handle: AppHandle) -> Result<(), String> {
    // Check if the mod daemon is already running
    let is_mod_daemon_running_ref = IS_MOD_DAEMON_RUNNING.clone();
    let mut is_mod_daemon_running = is_mod_daemon_running_ref.write().await;
    if *is_mod_daemon_running {
        return Ok(());
    }

    *is_mod_daemon_running = true;

    // Mod Daemon 👹
    // This task will run continuously and check if there are any mods in the download queue
    // to download. If there are, it will download them and remove them from the queue.
    // If there are no mods in the queue, it will sleep for a bit and check again.
    task::spawn(async move {
        let handle = app_handle.clone();

        loop {
            // How fast do we want to check the queue?
            time::sleep(Duration::from_millis(150)).await;

            // Now lets grab the front of that queue!
            // Keep in mind, we are grabbing the front, so we eventually have to put it back.
            // P.S. Sometimes the queue is empty!
            let mod_queue_ref = MOD_DOWNLOAD_QUEUE.clone();
            let mut mod_queue = mod_queue_ref.write().await;
            let front = mod_queue.pop_front();
            if front.is_none() {
                continue; // No mods in the queue, lets check again!
            }
            let front = front.unwrap();

            // Sometimes we can unmount steam while the daemon is running, so we
            // need to check if steamworks is still initialized! 🤭
            let client = client::get_client().await;
            if client.is_none() {
                println!("mdq_daemon: Steamworks not initialized, trying again!");
                continue;
            }
            let client = client.unwrap();

            // Lets check if the mod is installed?
            let ugc = client.ugc();
            let is_installed = ugc.item_install_info(steamworks::PublishedFileId(front));
            if is_installed.is_some() {
                println!(
                    "mdq_daemon: Mod has been installed: {}",
                    is_installed.unwrap().folder
                );

                // Emit an extra event to let the frontend know the mod is installed
                // Just in case the frontend is waiting for the download to finish
                let event = ActiveDownloadProgressEvent {
                    published_file_id: front,
                    bytes_downloaded: 0,
                    bytes_total: 0,
                    percentage_downloaded: 100.0,
                };

                handle
                    .emit("mdq_active_download_progress", event)
                    .expect("Failed to emit event!");
                continue;
            }

            // Its not installed, either we are downloading it or we need to download it!
            // Lets check our last download id...
            let download_info = ugc.item_download_info(steamworks::PublishedFileId(front));
            if download_info.is_some() {
                // Add it back and check again later
                mod_queue.push_front(front);

                // Gather the download info and send it to the frontend
                let bytes_downloaded = download_info.unwrap().0;
                let bytes_total = download_info.unwrap().1;
                let percentage_downloaded = bytes_downloaded as f64 / bytes_total as f64 * 100.0;

                let event = ActiveDownloadProgressEvent {
                    published_file_id: front,
                    bytes_downloaded,
                    bytes_total,
                    percentage_downloaded,
                };

                println!(
                    "mdq_daemon: {} is downloading... {:.1}% ({}/{})",
                    front, percentage_downloaded, bytes_downloaded, bytes_total
                );

                handle
                    .emit("mdq_active_download_progress", event)
                    .expect("Failed to emit event!");
                continue;
            }

            // At this point we know the mod is not installed and not downloading...
            // Lets cook that shit up 🍳 P.S. don't care about the callback
            ugc.subscribe_item(steamworks::PublishedFileId(front), |_i| {});
            mod_queue.push_front(front);
            println!("mdq_daemon: ✅ Downloading mod: {}", front);
        }
    });

    // Event Emitter 👷
    // This is responsible for keeping the frontend up to date about what mod is currently
    // downloading and its information.

    Ok(())
}

/**
* function: steam_remove_mod
* ---
* Unsubscribes from a mod from the Steamworks API.
*/
#[tauri::command]
pub async fn steam_remove_mod(published_file_id: u64) -> Result<(), String> {
    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Unsubscribe the mod
    let ugc = client.ugc();
    ugc.unsubscribe_item(
        steamworks::PublishedFileId(published_file_id),
        |i| match i {
            Ok(_) => println!("steam_remove_mod, mod unsubscribed successfully"),
            Err(e) => println!("Error unsubscribing mod: {}", e),
        },
    );

    Ok(())
}

/**
* function: steam_remove_mod_forcefully
* ---
* Unsubscribes and requests deletion of a mod from the Steamworks API.
*/
#[tauri::command]
pub async fn steam_remove_mod_forcefully(published_file_id: u64) -> Result<(), String> {
    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Unsubscribe and delete the mod
    let ugc = client.ugc();
    ugc.unsubscribe_item(
        steamworks::PublishedFileId(published_file_id),
        |i| match i {
            Ok(_) => println!("steam_remove_mod_forcefully: mod unsubscribed successfully"),
            Err(e) => println!("Error unsubscribing mod: {}", e),
        },
    );
    ugc.delete_item(
        steamworks::PublishedFileId(published_file_id),
        |i| match i {
            Ok(_) => {
                println!("steam_remove_mod_forcefully: requested deletion of mod successfully")
            }
            Err(e) => println!("Error deleting mod: {}", e),
        },
    );
    Ok(())
}

/**
* function: steam_fix_mod
* ---
* Tries to reset local mod cache and redownload the mod from the Steamworks API.
*/
#[tauri::command]
pub async fn steam_fix_mod(published_file_id: u64) -> Result<(), String> {
    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Check if the mod is installed before trying to download it
    let ugc = client.ugc();
    let install_info = ugc.item_install_info(steamworks::PublishedFileId(published_file_id));
    if install_info.is_none() {
        return Err("Mod is not installed! I can only fix installed mods.".to_string());
    }

    // Try to "download" the mod again, will verify the files
    let is_success = ugc.download_item(steamworks::PublishedFileId(published_file_id), true);

    match is_success {
        true => Ok(()),
        false => Err("Failed to download mod, you may have an invalid id.".to_string()),
    }
}

/**
* function: steam_fix_mod_forcefully
* ---
* Removes the mod's local files and redownloads the mod from the Steamworks API.
* ---
* WARN: This will remove all local files associated with the mod.
*/
#[tauri::command]
pub async fn steam_fix_mod_forcefully(
    published_file_id: u64,
    app_handle: AppHandle,
) -> Result<(), String> {
    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Make sure the mod is installed!
    let ugc = client.ugc();
    let install_info = ugc.item_install_info(steamworks::PublishedFileId(published_file_id));
    if install_info.is_none() {
        return Err("Mod is not installed! I can only fix installed mods.".to_string());
    }
    let path = install_info.unwrap().folder;

    // Delete that mod... 🤭
    println!(
        "steam_fix_mod_forcefully: ❌ Deleting mod... {}",
        published_file_id
    );
    fs::remove_dir_all(path).map_err(|e| e.to_string())?;

    // Now we download the mod again
    println!(
        "steam_fix_mod_forcefully: ✅ Re-downloading mod... {}",
        published_file_id
    );
    ugc.download_item(steamworks::PublishedFileId(published_file_id), true);

    // A short task to periodically check the download status
    task::spawn(async move {
        let handle = app_handle.clone();

        loop {
            time::sleep(Duration::from_millis(250)).await;

            // Get the client...
            let client = client::get_client().await;
            if client.is_none() {
                continue;
            }
            let client = client.unwrap();

            // Query the download status
            let ugc = client.ugc();
            let download_info =
                ugc.item_download_info(steamworks::PublishedFileId(published_file_id));
            if download_info.is_none() {
                panic!("⚠️ There was a serious error querying the download status!");
            }
            let download_info = download_info.unwrap();

            // Gather the data for the active download
            let bytes_downloaded = download_info.0;
            let bytes_total = download_info.1;
            let percentage_downloaded = bytes_downloaded as f64 / bytes_total as f64 * 100.0;
            let event = ActiveDownloadProgressEvent {
                published_file_id,
                bytes_downloaded,
                bytes_total,
                percentage_downloaded,
            };

            println!(
                "steam_fix_mod_forcefully: {} is downloading... {:.1}% ({}/{})",
                published_file_id, percentage_downloaded, bytes_downloaded, bytes_total
            );

            // Emit the event
            handle
                .emit("steam_fix_mod_forcefully_progress", event)
                .expect("Failed to emit progress event");

            // Make sure we're done
            if percentage_downloaded >= 100.0 {
                println!(
                    "steam_fix_mod_forcefully: {} has been redownloaded!",
                    published_file_id
                );
                break;
            }
        }
    });

    Ok(())
}

/**
* function: steam_get_missing_mods_for_server
* ---
* Queries the Steamworks API for a list of mods that are missing from the server.
* Must be given an array of mod ids to check against.
* Returns a list of mod ids that are missing.
*/
#[tauri::command]
pub async fn steam_get_missing_mods_for_server(
    required_mods: Vec<u64>,
) -> Result<Vec<u64>, String> {
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();
    let ugc = client.ugc();

    // Check if the mods are installed
    let mut missing_mods: Vec<u64> = Vec::new();
    for mod_id in required_mods {
        let install_info = ugc.item_install_info(steamworks::PublishedFileId(mod_id));
        if install_info.is_none() {
            missing_mods.push(mod_id);
        }
    }

    Ok(missing_mods)
}

/**
* function: steam_get_installed_mods
* ---
* Queries the Steamworks API for all installed mods and emits the results to the frontend.
* Emits a "steam_get_installed_mods_result" event for each mod found, with the mod's information.
*/
#[tauri::command]
pub async fn steam_get_installed_mods(app_handle: AppHandle) -> Result<(), String> {
    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Get the installed mods
    let ugc = client.ugc();
    let subscribed_items = ugc.subscribed_items();
    for item in subscribed_items {
        let extended_info = ugc.query_item(item).map_err(|e| e.to_string())?;

        // We only want DOWNLOADED mods
        let path = ugc.item_install_info(item);
        if path.is_none() {
            continue;
        }
        let path = path.unwrap().folder;

        let handle = app_handle.clone();
        extended_info.fetch(move |i| {
            let query_result = i.unwrap().get(0).unwrap();
            let size = get_size(&path).unwrap();

            let result = ModInfo {
                published_file_id: query_result.published_file_id.0,
                title: query_result.title,
                description: query_result.description,
                owner_steam_id: query_result.owner.raw(),
                time_created: query_result.time_created,
                time_updated: query_result.time_updated,
                time_added_to_user_list: query_result.time_added_to_user_list,
                banned: query_result.banned,
                accepted_for_use: query_result.accepted_for_use,
                tags: query_result.tags.clone(),
                tags_truncated: query_result.tags_truncated,
                file_size: size as u32,
                url: query_result.url.clone(),
                num_upvotes: query_result.num_upvotes,
                num_downvotes: query_result.num_downvotes,
                score: query_result.score,
                num_children: query_result.num_children,
            };

            handle
                .emit("steam_get_installed_mods_result", result)
                .expect("Failed to emit query result");
        });
    }

    Ok(())
}

/**
* function: steam_get_mod_info
* ---
* Queries the Steamworks API for a specific mod's information and emits the results to the frontend.
* Emits a "steam_get_mod_info_result" event with the mod's information.
* ---
* NOTE: Untested, but should work.
*/
#[tauri::command]
pub async fn steam_get_mod_info(
    app_handle: AppHandle,
    published_file_id: u64,
) -> Result<(), String> {
    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Get the mod info
    let ugc = client.ugc();
    let extended_info = ugc
        .query_item(steamworks::PublishedFileId(published_file_id))
        .map_err(|e| e.to_string())?;

    extended_info.fetch(move |i| {
        let query_result = i.unwrap().get(0).unwrap();

        let result = ModInfo {
            published_file_id: query_result.published_file_id.0,
            title: query_result.title,
            description: query_result.description,
            owner_steam_id: query_result.owner.raw(),
            time_created: query_result.time_created,
            time_updated: query_result.time_updated,
            time_added_to_user_list: query_result.time_added_to_user_list,
            banned: query_result.banned,
            accepted_for_use: query_result.accepted_for_use,
            tags: query_result.tags.clone(),
            tags_truncated: query_result.tags_truncated,
            file_size: query_result.file_size,
            url: query_result.url.clone(),
            num_upvotes: query_result.num_upvotes,
            num_downvotes: query_result.num_downvotes,
            score: query_result.score,
            num_children: query_result.num_children,
        };

        app_handle
            .emit("steam_get_mod_info_result", result)
            .expect("Failed to emit query result");
    });

    Ok(())
}

/**
* function: steam_get_user_display_name
* ---
* Retrieves the current user's display name from the Steamworks API.
* Will error if no steam client is found.
*/
#[tauri::command]
pub async fn steam_get_user_display_name() -> Result<String, String> {
    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Grab that name!
    Ok(client.friends().name())
}

/**
* function: steam_get_user_id
* ---
* Retrieves the current user's Steam 64 ID from the Steamworks API.
* Will error if no steam client is found.
*/
#[tauri::command]
pub async fn steam_get_user_id() -> Result<String, String> {
    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Grab that ID!
    Ok(client.user().steam_id().raw().to_string())
}

/**
* function: steam_get_user_avi
* --------------------------
* Queries the Steamworks API for the current user's avatar and returns it as a byte array.
* RGBA format. Will error if no steam client is found.
*/
#[tauri::command]
pub async fn steam_get_user_avi() -> Result<Vec<u8>, String> {
    // Check that steam client!
    let client = client::get_client().await;
    if client.is_none() {
        return Err("No steam client found!".to_string());
    }
    let client = client.unwrap();

    // Get the avatar!
    let avi = client.friends().medium_avatar();
    match avi {
        Some(avi) => Ok(avi),
        None => Err("No avatar found 😥".to_string()),
    }
}

/**
* function: steam_start_daemon
* ---
* Starts a daemon that runs Steamworks callbacks every 50ms.
* We can start this deamon before starting steamworks, as it will
* continually check if the client is available.
*/
#[tauri::command]
pub async fn steam_start_daemon() -> Result<(), String> {
    // Check if we are already running the callback daemon!
    let is_callback_daemon_running_ref = IS_CALLBACK_DAEMON_RUNNING.clone();
    let mut is_callback_daemon_running = is_callback_daemon_running_ref.write().await;
    if *is_callback_daemon_running {
        return Ok(());
    }

    task::spawn(async {
        loop {
            // Time to sleep before trying again
            time::sleep(Duration::from_millis(50)).await;

            // If we currently don't have a client, retry!
            if !client::has_client().await {
                continue;
            }

            // Get the client every time?
            // This is because the client might be dropped and recreated
            let single = client::get_single();
            single.run_callbacks();
        }
    });

    *is_callback_daemon_running = true;
    Ok(())
}

/**
* function: steam_mount_api
* ---
* Initializes the Steamworks API. This function must be called before any other Steamworks functions.
* Can error if the is already mounted, or has an incorrect app id.
*/
#[tauri::command]
pub async fn steam_mount_api() -> Result<(), String> {
    // Get the base reference to the client
    let client_ref = client::STEAM_CLIENT.clone();
    let mut client_ref = client_ref.lock_owned().await;

    // Client is already initialized
    if client_ref.is_some() {
        return Ok(());
    }

    // Mount API with DayZ app id
    let result = steamworks::Client::init_app(221100);

    match result {
        Ok(client) => {
            let (client, single) = client;

            // Manually set the client and single
            *client_ref = Some(client);
            unsafe {
                client::STEAM_SINGLE = Some(single);
            }
            Ok(())
        }
        Err(e) => {
            println!("Error initializing Steamworks: {}", e);
            Err(e.to_string())
        }
    }
}

/**
* function: steam_unmount_api
* ---
* Destructures the Steamworks API. Does not *need* to be called, but can be useful forcing Steam
* to think that we have shutdown and the "game" has been closed.
* Can error if the is already mounted, or has an incorrect app id.
* ---
* WARN: This function is inherently unsafe! Please use with caution.
*/
#[tauri::command]
pub async fn steam_unmount_api() -> Result<(), String> {
    // Get the base reference to the client
    let client_ref = client::STEAM_CLIENT.clone();
    let mut client_ref = client_ref.lock_owned().await;

    // We don't have a client to unmount
    if client_ref.is_none() {
        return Ok(());
    }

    // Run some last callbacks!
    unsafe {
        client::STEAM_SINGLE.as_ref().unwrap().run_callbacks();
    }

    // Manually set the client and single
    *client_ref = None;
    unsafe {
        client::STEAM_SINGLE = None;
    }

    // Now we nuke the API 💣
    println!("Shutting down Steamworks API");
    steamworks::Client::shutdown();

    // Now we have to quickly remount and unmount the api to 480
    // As well as run the callbacks, so we don't get any errors
    let e = steamworks::Client::init_app(0);
    // print the error form E
    match e {
        Err(e) => {
            println!("Error initializing Steamworks: {}", e);
            return Err(e.to_string());
        }
        Ok(_) => {}
    }

    steamworks::Client::shutdown();

    Ok(())
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct ModInfo {
    published_file_id: u64,
    title: String,
    description: String,
    owner_steam_id: u64,
    time_created: u32,
    time_updated: u32,
    time_added_to_user_list: u32,
    banned: bool,
    accepted_for_use: bool,
    tags: Vec<String>,
    tags_truncated: bool,
    file_size: u32,
    url: String,
    num_upvotes: u32,
    num_downvotes: u32,
    score: f32,
    num_children: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct ActiveDownloadProgressEvent {
    published_file_id: u64,
    bytes_downloaded: u64,
    bytes_total: u64,
    percentage_downloaded: f64,
}
