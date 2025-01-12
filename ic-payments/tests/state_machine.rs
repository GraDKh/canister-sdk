#[cfg(feature = "state-machine")]
mod tests {
    use candid::{CandidType, Decode, Deserialize, Encode, Nat, Principal};
    use ic_exports::ic_base_types::{CanisterId, PrincipalId};
    use ic_exports::ic_icrc1::endpoints::{TransferArg, TransferError, Value};
    use ic_exports::ic_icrc1::Account;
    use ic_exports::ic_kit::mock_principals::{alice, bob};
    use ic_exports::ic_state_machine_tests::StateMachine;
    use ic_helpers::tokens::Tokens128;
    use ic_payments::error::{PaymentError, TransferFailReason};
    use ic_payments::get_principal_subaccount;

    #[derive(CandidType, Clone, Debug)]
    pub struct InitArgs {
        pub minting_account: Account,
        pub initial_balances: Vec<(Account, u64)>,
        pub transfer_fee: u64,
        pub token_name: String,
        pub token_symbol: String,
        pub metadata: Vec<(String, Value)>,
        pub archive_options: ArchiveOptions,
    }

    #[derive(Deserialize, CandidType, Clone, Debug, PartialEq, Eq, Default)]
    pub struct ArchiveOptions {
        /// The number of blocks which, when exceeded, will trigger an archiving
        /// operation
        pub trigger_threshold: usize,
        /// The number of blocks to archive when trigger threshold is exceeded
        pub num_blocks_to_archive: usize,
        pub node_max_memory_size_bytes: Option<usize>,
        pub max_message_size_bytes: Option<usize>,
        pub controller_id: PrincipalId,
        // cycles to use for the call to create a new archive canister
        #[serde(default)]
        pub cycles_for_archive_creation: Option<u64>,
        // Max transactions returned by the [get_transactions] endpoint
        #[serde(default)]
        pub max_transactions_per_response: Option<usize>,
    }

    fn token_wasm() -> &'static [u8] {
        include_bytes!("./common/ic-icrc1-ledger.wasm")
    }

    fn payment_canister_wasm() -> &'static [u8] {
        include_bytes!("./common/payment_canister.wasm")
    }

    const INIT_BALANCE: u128 = 10u128.pow(12);

    fn init_token(env: &mut StateMachine) -> CanisterId {
        let args = InitArgs {
            minting_account: Account {
                owner: alice().into(),
                subaccount: None,
            },
            initial_balances: vec![(
                Account {
                    owner: bob().into(),
                    subaccount: None,
                },
                INIT_BALANCE as u64,
            )],
            transfer_fee: 100,
            token_name: "Icrcirium".into(),
            token_symbol: "ICRC".into(),
            metadata: vec![],
            archive_options: ArchiveOptions::default(),
        };
        let args = Encode!(&args, &Tokens128::from(1_000_000_000)).unwrap();
        let principal = env
            .install_canister(token_wasm().into(), args, None)
            .expect("failed to install token canister");

        eprintln!("Created token canister {principal}");
        principal
    }

    fn init_payment(env: &mut StateMachine, token: Principal) -> CanisterId {
        let args = Encode!(&token).unwrap();
        let principal = env
            .install_canister(payment_canister_wasm().into(), args, None)
            .expect("failed to install payment canister");

        eprintln!("Created payment canister {principal}");
        principal
    }

    fn get_token_principal_balance(
        env: &StateMachine,
        token: CanisterId,
        of: Principal,
    ) -> Option<Nat> {
        let account = Account {
            owner: of.into(),
            subaccount: None,
        };
        let payload = Encode!(&account).unwrap();
        let response = env
            .execute_ingress(token, "icrc1_balance_of", payload)
            .unwrap();
        Decode!(&response.bytes(), Option<Nat>).unwrap()
    }

    #[test]
    fn terminal_operations() {
        let mut env = StateMachine::new();
        let token = init_token(&mut env);
        let payment = init_payment(&mut env, token.get().into());
        env.add_cycles(payment, 10u128.pow(15));

        let payload = Encode!(&()).unwrap();
        env.execute_ingress(payment, "configure", payload).unwrap();

        let payload = Encode!(&Tokens128::from(1_000_000)).unwrap();
        let response = env
            .execute_ingress_as(bob().into(), payment, "deposit", payload)
            .unwrap();
        let decoded = Decode!(&response.bytes(), Result<(Nat, Tokens128), PaymentError>).unwrap();

        assert_eq!(
            decoded,
            Err(PaymentError::TransferFailed(TransferFailReason::Rejected(
                TransferError::InsufficientFunds { balance: 0.into() }
            )))
        );

        let subaccount = get_principal_subaccount(bob());
        let payload = Encode!(&TransferArg {
            from_subaccount: None,
            to: Account {
                owner: payment.into(),
                subaccount
            },
            fee: None,
            created_at_time: None,
            memo: None,
            amount: 2_000_000.into()
        })
        .unwrap();
        let response = env
            .execute_ingress_as(bob().into(), token, "icrc1_transfer", payload)
            .unwrap();
        Decode!(&response.bytes(), Result<Nat, TransferError>)
            .unwrap()
            .unwrap();

        let payload = Encode!(&Tokens128::from(2_000_000)).unwrap();
        let response = env
            .execute_ingress_as(bob().into(), payment, "deposit", payload)
            .unwrap();
        let (_, transferred) = Decode!(&response.bytes(), Result<(Nat, Tokens128), PaymentError>)
            .unwrap()
            .unwrap();
        assert_eq!(transferred, 1_999_900.into());

        let payload = Encode!(&()).unwrap();
        let response = env
            .execute_ingress_as(bob().into(), payment, "get_balance", payload)
            .unwrap();
        let (local_balance, token_balance) =
            Decode!(&response.bytes(), Tokens128, Tokens128).unwrap();

        assert_eq!(local_balance, 1_999_900.into());
        assert_eq!(token_balance, 1_999_900.into());

        let payload = Encode!(&Tokens128::from(1_999_900)).unwrap();
        let response = env
            .execute_ingress_as(bob().into(), payment, "withdraw", payload)
            .unwrap();
        let (_, transferred) = Decode!(&response.bytes(), Result<(Nat, Tokens128), PaymentError>)
            .unwrap()
            .unwrap();
        assert_eq!(transferred, 1_999_700.into());

        let user_balance = get_token_principal_balance(&env, token, bob()).unwrap();
        let canister_balance =
            get_token_principal_balance(&env, token, payment.get().into()).unwrap_or_default();

        const FEES: u128 = 100 * 4;
        assert_eq!(user_balance, Nat::from(INIT_BALANCE - FEES));
        assert_eq!(canister_balance, Nat::from(0));
    }
}
