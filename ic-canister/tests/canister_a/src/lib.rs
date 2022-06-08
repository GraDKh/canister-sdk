use candid::{CandidType, Deserialize, Principal};
use ic_storage::stable::Versioned;
use ic_storage::IcStorage;
use std::cell::RefCell;
use std::rc::Rc;

use ic_canister::{generate_exports, query, update, Canister};

#[derive(Default, CandidType, Deserialize, IcStorage)]
pub struct State {
    counter: u32,
}

impl Versioned for State {
    type Previous = ();

    fn upgrade((): ()) -> Self {
        Self::default()
    }
}

pub trait CanisterATrait {
    fn state(&self) -> Rc<RefCell<State>>;

    #[query(trait = true)]
    fn get_counter(&self) -> u32 {
        self.state().borrow().counter
    }

    #[update(trait = true)]
    fn inc_counter(&mut self, value: u32) {
        RefCell::borrow_mut(&self.state()).counter += value;
    }
}

#[derive(Clone, Canister)]
pub struct CanisterA {
    #[id]
    principal: Principal,
    #[state]
    state: Rc<RefCell<State>>,
}

impl CanisterATrait for CanisterA {
    fn state(&self) -> Rc<RefCell<State>> {
        self.state.clone()
    }
}

generate_exports!(CanisterA);
