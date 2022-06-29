use super::checksum::Version;
use candid::utils::ArgumentEncoder;
use candid::{CandidType, Principal};
use ic_cdk::api::call::CallResult;
use ic_helpers::management::Canister as ManagementCanister;
use ic_helpers::management::InstallCodeMode;
use serde::{Deserialize, Serialize};

/// Represents information of a canister.
#[derive(CandidType, Debug, Clone, Serialize, Deserialize)]
#[deprecated]
pub struct Canister(Principal, Version);

impl Canister {
    pub fn new(id: Principal, version: Version) -> Self {
        Self(id, version)
    }

    /// Returns the version of module that is installed on canister.
    pub fn version(&self) -> Version {
        self.1
    }

    /// Returns the principal id of canister.
    pub fn identity(&self) -> Principal {
        self.0
    }

    /// Creates a new canister and installs `wasm_module` on it.
    pub async fn create<A: ArgumentEncoder>(
        version: Version,
        wasm_module: Vec<u8>,
        arg: A,
        cycles: u64,
    ) -> CallResult<Self> {
        let canister = ManagementCanister::create(None, cycles).await?;
        canister
            .install_code(InstallCodeMode::Install, wasm_module, arg)
            .await?;
        Ok(Self(canister.into(), version))
    }

    /// Upgrades the canister.
    pub async fn upgrade(&mut self, version: Version, wasm_module: Vec<u8>) -> CallResult<()> {
        ManagementCanister::from(self.0)
            .install_code(InstallCodeMode::Upgrade, wasm_module, ())
            .await?;
        self.1 = version;
        Ok(())
    }
}
