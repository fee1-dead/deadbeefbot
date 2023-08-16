use std::num::NonZeroUsize;

use parsoid::map::IndexMap;

pub trait AddToParams {
    fn add_to_params(self, i: NonZeroUsize, params: &mut ParamBuilder<'_>);
}

pub struct ParamBuilder<'a> {
    params: &'a mut IndexMap<String, String>,
}

impl<'a> ParamBuilder<'a> {
    pub fn new(params: &'a mut IndexMap<String, String>) -> Self {
        ParamBuilder { params }
    }

    pub fn add(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.params.insert(key.into(), value.into());
        self
    }

    pub fn add_opt(
        &mut self,
        key: impl Into<String>,
        value: Option<impl Into<String>>,
    ) -> &mut Self {
        if let Some(value) = value {
            self.params.insert(key.into(), value.into());
        }
        self
    }

    pub fn add_all(&mut self, params: impl IntoIterator<Item = impl AddToParams>) -> &mut Self {
        params
            .into_iter()
            .enumerate()
            .for_each(|(i, p)| p.add_to_params(NonZeroUsize::new(i + 1).unwrap(), self));
        self
    }

    pub fn add_flag(&mut self, key: impl Into<String>, flag: bool) -> &mut Self {
        if flag {
            self.params.insert(key.into(), "yes".into());
        }
        self
    }

    pub fn newline(&mut self) {
        self.params
            .last_mut()
            .unwrap()
            .1
            .push_str("{{subst:User:0xDeadbeef/newline}}")
    }
}
