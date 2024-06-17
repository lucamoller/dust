use crate::Executor;
use crate::ValueToIdentifier;
use crate::{ArgState, CallbackId, ExecutionArg};
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
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
        execution_args: Vec<ExecutionArg<Self::V>>,
        server_plan: Vec<CallbackId>,
    ) -> impl std::future::Future<Output = Result<Vec<ExecutionArg<Self::V>>, ::leptos::ServerFnError>>
           + Send;

    fn get_values_from_identifiers(
        &self,
        identifiers: &std::collections::HashSet<Self::I>,
    ) -> Vec<Self::V> {
        identifiers
            .iter()
            .map(|identifier| self.read_value(identifier))
            .collect()
    }

    fn apply_updates(&self, updated_state: HashMap<Self::I, ExecutionArg<Self::V>>) {
        for (_, arg) in updated_state {
            match arg.state {
                ArgState::Updated => {
                    self.update_value(arg.value);
                },
                ArgState::Unmodified => {}
            };
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
        let updated_inputs: HashSet<Self::I> =
            input_updates.iter().map(|v| v.to_identifier()).collect();
        let execution_plan = Self::get_executor().get_execution_plan(&updated_inputs);

        let mut updated_state: HashMap<Self::I, ExecutionArg<Self::V>> = HashMap::new();
        for value in input_updates.iter() {
            let identifier = value.to_identifier();
            updated_state.insert(
                identifier,
                ExecutionArg {
                    value: value.clone(),
                    state: ArgState::Updated,
                },
            );
        }

        if !execution_plan.client_pre_plan.is_empty() {
            let client_pre_args =
                self.get_execution_args(&updated_state, &execution_plan.client_pre_plan);
            ::leptos::logging::log!("  client_pre_args: {:?}", &client_pre_args);
            ::leptos::logging::log!("  client_pre_plan: {:?}", &execution_plan.client_pre_plan);
            let client_pre_output = Self::get_executor()
                .execute_plan(&client_pre_args, &execution_plan.client_pre_plan);
            ::leptos::logging::log!("  client_pre_output: {:?}", &client_pre_output);
            for arg in client_pre_output.iter() {
                let identifier = arg.value.to_identifier();
                updated_state.insert(identifier, arg.clone());
            }
        } else {
            ::leptos::logging::log!("  no client_pre_plan");
        }

        let context = self.clone();
        ::leptos::spawn_local(async move {
            if !execution_plan.server_plan.is_empty() {
                let server_args =
                    context.get_execution_args(&updated_state, &execution_plan.server_plan);
                ::leptos::logging::log!("  server_args: {:?}", &server_args);
                ::leptos::logging::log!("  server_plan: {:?}", &execution_plan.server_plan);
                let response =
                    Self::execute_server_callbacks(server_args, execution_plan.server_plan.clone())
                        .await;
                let server_output = match response {
                    Ok(server_output) => server_output,
                    Err(e) => {
                        ::leptos::logging::log!("server_callback error: {}", e);
                        return;
                    }
                };
                ::leptos::logging::log!("    server_output: {:?}", server_output);
                for arg in server_output.iter() {
                    let identifier = arg.value.to_identifier();
                    updated_state.insert(identifier, arg.clone());
                }
            } else {
                ::leptos::logging::log!("  no server_plan");
            }

            if !execution_plan.client_post_plan.is_empty() {
                let client_post_args =
                    context.get_execution_args(&updated_state, &execution_plan.client_post_plan);
                ::leptos::logging::log!("  client_post_args: {:?}", &client_post_args);
                ::leptos::logging::log!(
                    "  client_post_plan: {:?}",
                    &execution_plan.client_post_plan
                );
                let client_post_output = Self::get_executor()
                    .execute_plan(&client_post_args, &execution_plan.client_post_plan);
                ::leptos::logging::log!("  client_post_output: {:?}", &client_post_output);
                for arg in client_post_output.iter() {
                    let identifier = arg.value.to_identifier();
                    updated_state.insert(identifier, arg.clone());
                }
            } else {
                ::leptos::logging::log!("  no client_post_plan");
            }

            context.apply_updates(updated_state);
        });
    }

    fn get_execution_args(
        &self,
        updated_state: &HashMap<Self::I, ExecutionArg<Self::V>>,
        plan: &Vec<CallbackId>,
    ) -> Vec<ExecutionArg<Self::V>> {
        let mut execution_args: Vec<ExecutionArg<Self::V>> = Vec::new();

        let required_args = Self::get_executor().get_required_args(plan);

        for identifier in required_args.iter() {
            match updated_state.get(identifier) {
                Some(arg) => {
                    execution_args.push(arg.clone());
                }
                None => {
                    execution_args.push(ExecutionArg {
                        value: self.read_value(identifier).clone(),
                        state: ArgState::Unmodified,
                    });
                }
            };
        }
        return execution_args;
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
