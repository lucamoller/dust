use std::collections::HashMap;

pub struct Input<T> {
    pub value: T,
}

#[derive(Clone, Copy)]
pub enum OutputState<T> {
    NoChange,
    Updated(T),
}

pub struct Output<T> {
    pub state: OutputState<T>,
}

impl<T: Clone> Output<T> {
    pub fn new() -> Output<T> {
        Output {
            state: OutputState::NoChange,
        }
    }

    pub fn set(&mut self, value: T) {
        self.state = OutputState::Updated(value);
    }
}

#[derive(Clone, PartialEq)]
pub enum CallbackType {
    Server,
    Client,
}

#[derive(Clone)]
pub struct Callback<I, V> {
    pub name: &'static str,
    pub cb: Option<fn(&HashMap<I, V>) -> Vec<V>>,
    pub inputs: Vec<I>,
    pub outputs: Vec<I>,
    pub cb_type: CallbackType,
}

impl<I, V> Callback<I, V> {
    pub fn new(
        name: &'static str,
        cb: Option<fn(&HashMap<I, V>) -> Vec<V>>,
        inputs: Vec<I>,
        outputs: Vec<I>,
        cb_type: CallbackType,
    ) -> Self {
        Self {
            name,
            cb,
            inputs,
            outputs,
            cb_type,
        }
    }
}

impl<I, V> std::hash::Hash for Callback<I, V> {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.name.hash(state);
        state.finish();
    }
}

impl<I, V> PartialEq for Callback<I, V> {
    fn eq(&self, other: &Callback<I, V>) -> bool {
        return self.name == other.name;
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CallbackId(pub usize);

impl CallbackId {
    pub fn callback_index(&self) -> usize {
        // Returns the index of the callback in the Executor's callbacks list.
        return self.0;
    }
}

#[derive(Clone)]
pub struct CallbackWithId<I, V> {
    pub id: CallbackId,
    pub callback: Callback<I, V>,
}

pub trait StateTypes {
    type Identifier;
    type Value;
}

impl<I, V> Eq for Callback<I, V> {}

pub trait ValueToIdentifier<I> {
    fn to_identifier(&self) -> I;
}