use crate::Executor;
use crate::ValueToIdentifier;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::rc::Rc;

pub trait ContextProvider {
    type C: Context;

    fn provide_and_initialize_context() {
        let context = Rc::new(Self::C::from_default_state());
        ::leptos::provide_context(context.clone());
        ::leptos::create_effect(move |_| {
            ::leptos::logging::log!("Initializing state...");
            context.initialize_context();
        });
    }

    fn expect_context() -> Rc<Self::C> {
        return ::leptos::expect_context::<Rc<Self::C>>();
    }
}

pub trait Context
where
    Self: 'static,
{
    type I: Hash + PartialEq + Eq + Clone + Copy + 'static; // Identifier enum
    type V: Clone + Debug + ValueToIdentifier<Self::I> + 'static; // Value enum

    fn from_default_state() -> Self;
    fn update_value(&self, value: Self::V);
    fn read_value(&self, identifier: &Self::I) -> Self::V;
    fn get_manager(&self) -> &ContextManager<Self::I>;
    fn get_executor() -> &'static Executor<Self::I, Self::V>;

    fn execute_server_callbacks(
        input_updates: Vec<Self::V>,
        required_state: Vec<Self::V>,
    ) -> impl std::future::Future<Output = Result<Vec<Self::V>, ::leptos::ServerFnError>> + Send;

    fn get_values_from_identifiers(
        &self,
        identifiers: &std::collections::HashSet<Self::I>,
    ) -> Vec<Self::V> {
        identifiers
            .iter()
            .map(|identifier| self.read_value(identifier))
            .collect()
    }

    fn apply_updates(&self, updates: Vec<Self::V>) {
        for update in updates {
            self.update_value(update);
        }
    }

    fn initialize_context(self: &std::rc::Rc<Self>) {
        ::leptos::logging::log!("initialize_context");
        self.get_manager().initialized.set(true);
        self.handle_updates(self.get_values_from_identifiers(
            &Self::get_executor().get_required_initialization_inputs(),
        ));
    }

    fn handle_updates(self: &std::rc::Rc<Self>, input_updates: Vec<Self::V>) {
        let updated_inputs = input_updates.iter().map(|v| v.to_identifier()).collect();
        let execution_plan = Self::get_executor().get_execution_plan(&updated_inputs);
        let required_state =
            Self::get_executor().get_required_state(&updated_inputs, &execution_plan);
        let required_state_values = self.get_values_from_identifiers(&required_state);

        ::leptos::logging::log!("handle_updates call");
        ::leptos::logging::log!("  input_updates: {:?}", input_updates);
        ::leptos::logging::log!("  execution_plan: {:?}", execution_plan);
        ::leptos::logging::log!("  required_state_values: {:?}", required_state_values);

        let state = self.clone();
        ::leptos::spawn_local(async move {
            let response =
                Self::execute_server_callbacks(input_updates, required_state_values).await;
            match response {
                Ok(output_updates) => {
                    ::leptos::logging::log!("    server output_updates: {:?}", output_updates);
                    state.apply_updates(output_updates);
                }
                Err(e) => {
                    ::leptos::logging::log!("server_callback error: {}", e);
                }
            }
        });
    }
}

#[derive(Debug)]
pub struct ContextManager<I> {
    pub initialized: Cell<bool>,
    pub value_versions: RefCell<HashMap<I, u32>>,
}

impl<I> ContextManager<I> {
    pub fn new() -> ContextManager<I> {
        return ContextManager {
            initialized: Cell::new(false),
            value_versions: RefCell::new(HashMap::new()),
        };
    }
}
