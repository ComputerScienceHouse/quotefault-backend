use std::string::ToString;

pub struct SearchAttrs {
    attrs: Vec<String>,
}

impl Default for SearchAttrs {
    fn default() -> Self {
        SearchAttrs {
            attrs: vec![
                String::from("cn"),
                String::from("dn"),
                String::from("uid"),
                String::from("ritDn"),
                String::from("memberOf"),
                String::from("krbPrincipalName"),
                String::from("mail"),
                String::from("mobile"),
                String::from("ibutton"),
                String::from("drinkBalance"),
            ],
        }
    }
}

impl SearchAttrs {
    pub fn new(attrs: &[&str]) -> Self {
        SearchAttrs {
            attrs: attrs.iter().map(ToString::to_string).collect(),
        }
    }

    #[must_use]
    pub fn add(mut self, attr: &str) -> Self {
        if !(self.attrs.contains(&attr.to_string())) {
            self.attrs.push(attr.to_string());
        }
        self
    }

    #[must_use]
    pub fn remove(mut self, attr: &str) -> Self {
        if attr != "dn" {
            self.attrs.retain(|a| a != attr);
        }
        self
    }

    #[must_use]
    pub fn finalize(self) -> Vec<String> {
        self.attrs
    }
}
