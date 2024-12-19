use postgrest::Postgrest;
use serde::Deserialize;
use agave_geyser_plugin_interface::geyser_plugin_interface::GeyserPluginError;
use agave_geyser_plugin_interface::geyser_plugin_interface::{
    GeyserPlugin, ReplicaAccountInfoVersions, Result as PluginResult,
};
use std::{
    error::Error,
    fmt::{self, Debug},
    fs::OpenOptions,
    io::Read,
    sync::Arc,
};
use tokio::runtime::Runtime;

pub struct PostgresPlugin {
    postgres_client: Arc<Option<Postgrest>>,
    configuration: Arc<Option<Configuration>>,
    programs: Arc<Vec<[u8; 32]>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Configuration {
    pub supabase_url: String,
    pub supabase_key: String,
    pub programs: Option<Vec<String>>,
}

impl Configuration {
    pub fn load(config_path: &str) -> Result<Self, Box<dyn Error>> {
        let mut file = OpenOptions::new().read(true).open(config_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        Ok(serde_json::from_str::<Configuration>(&contents)?)
    }
}

impl Default for PostgresPlugin {
    fn default() -> Self {
        PostgresPlugin {
            postgres_client: Arc::new(None),
            configuration: Arc::new(None),
            programs: Arc::new(Vec::new()),
        }
    }
}

impl GeyserPlugin for PostgresPlugin {
    fn name(&self) -> &'static str {
        "geyser"
    }

    fn on_load(&mut self, config_file: &str, _is_startup: bool) -> PluginResult<()> {
        println!("config file: {}", config_file);
        let config = match Configuration::load(config_file) {
            Ok(c) => c,
            Err(_e) => {
                return Err(GeyserPluginError::ConfigFileReadError {
                    msg: String::from("Error opening, or reading config file"),
                });
            }
        };
        println!("Your supabase url: {:#?} ", &config.supabase_url);
        
        // Create new Arcs with the updated values
        self.postgres_client = Arc::new(Some(
            Postgrest::new(&config.supabase_url).insert_header("apikey", &config.supabase_key)
        ));

        let mut programs = Vec::new();
        if let Some(accounts) = config.programs.as_ref() {
            for account in accounts {
                let mut acc_bytes = [0u8; 32];
                acc_bytes.copy_from_slice(&bs58::decode(account).into_vec().unwrap()[0..32]);
                programs.push(acc_bytes);
            }
        }
        self.programs = Arc::new(programs);
        self.configuration = Arc::new(Some(config));
        Ok(())
    }

    fn on_unload(&mut self) {}

    fn update_account(
        &self,
        account: ReplicaAccountInfoVersions,
        _slot: u64,
        _is_startup: bool,
    ) -> PluginResult<()> {
        let account_info = match account {
            ReplicaAccountInfoVersions::V0_0_1(_) => {
                return Err(GeyserPluginError::AccountsUpdateError {
                    msg: "V1 not supported, please upgrade your Solana CLI Version".to_string(),
                });
            },
            ReplicaAccountInfoVersions::V0_0_2(_account_info) => {
                return Err(GeyserPluginError::AccountsUpdateError {
                    msg: "V2 not supported, please upgrade your Solana CLI Version".to_string(),
                 })
            },
            ReplicaAccountInfoVersions::V0_0_3(account_info) => account_info,
        };

        let programs = Arc::clone(&self.programs);
        let postgres_client = Arc::clone(&self.postgres_client);

        for program in programs.iter() {
            if program == account_info.owner {
                let account_pubkey = bs58::encode(account_info.pubkey).into_string();
                let account_owner = bs58::encode(account_info.owner).into_string();
                let account_data = account_info.data;
                let _account_lamports = account_info.lamports;
                let account_executable = account_info.executable;
                let _account_rent_epoch = account_info.rent_epoch;

                let rt = Runtime::new().unwrap();
                if let Some(client) = &*postgres_client {
                    let result = rt.block_on(
                        client
                            .from("accounts")
                            .upsert(
                                serde_json::to_string(
                                    &serde_json::json!([{
                                        "account": account_pubkey,
                                        "owner": account_owner,
                                        "data": account_data,
                                        "executable": account_executable
                                    }]),
                                )
                                .unwrap(),
                            )
                            .execute(),
                    );
                    // account CRCJ7zzd5SSmA8AJ9gbtv4QrYZ2zw4YKWa1MCDw1NTf2 updated at slot 4!
                    match result {
                        Ok(_) => {
                            println!(
                                "account {} updated at slot {}!",
                                account_pubkey, _slot
                            );
                        }
                        Err(e) => {
                            println!("Error updating account: {:?}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }
    fn notify_end_of_startup(&self) -> PluginResult<()> {
        Ok(())
    }

    fn account_data_notifications_enabled(&self) -> bool {
        true
    }

    fn transaction_notifications_enabled(&self) -> bool {
        false
    }
}

impl Debug for PostgresPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresPlugin")
            .field("postgres_client", &self.postgres_client.as_ref().is_some())
            .finish()
    }
}