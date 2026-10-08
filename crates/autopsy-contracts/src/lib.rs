//! autopsy-contracts: Normalized callable and interface contract models and compatibility rules (FR-009, FR-010).
//!
//! Provides normalized contract models for TypeScript and other languages,
//! deterministic BLAKE3 contract digests, and formal compatibility evaluation rules per kind.

use autopsy_domain::{Contract, SymbolId, Visibility};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("Incompatible contract kinds: before is {before_kind}, after is {after_kind}")]
    KindMismatch {
        before_kind: &'static str,
        after_kind: &'static str,
    },
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Normalized parameter specification for callable contracts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ParameterModel {
    pub name: String,
    pub type_annotation: Option<String>,
    pub is_optional: bool,
    pub has_default: bool,
}

impl ParameterModel {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_annotation: None,
            is_optional: false,
            has_default: false,
        }
    }

    pub fn with_type(mut self, ty: impl Into<String>) -> Self {
        self.type_annotation = Some(ty.into());
        self
    }

    pub fn optional(mut self) -> Self {
        self.is_optional = true;
        self
    }

    pub fn with_default(mut self) -> Self {
        self.has_default = true;
        self
    }
}

/// Normalized callable contract (functions, methods, constructors).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CallableContract {
    pub owner: SymbolId,
    pub name: String,
    pub visibility: Visibility,
    pub parameters: Vec<ParameterModel>,
    pub return_type: Option<String>,
    pub type_parameters: Vec<String>,
    pub is_async: bool,
}

/// Normalized property specification for interface or type contracts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PropertyModel {
    pub name: String,
    pub type_annotation: Option<String>,
    pub is_optional: bool,
    pub is_readonly: bool,
}

impl PropertyModel {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_annotation: None,
            is_optional: false,
            is_readonly: false,
        }
    }

    pub fn with_type(mut self, ty: impl Into<String>) -> Self {
        self.type_annotation = Some(ty.into());
        self
    }

    pub fn optional(mut self) -> Self {
        self.is_optional = true;
        self
    }

    pub fn readonly(mut self) -> Self {
        self.is_readonly = true;
        self
    }
}

/// Normalized interface or object type contract.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InterfaceContract {
    pub owner: SymbolId,
    pub name: String,
    pub visibility: Visibility,
    pub properties: BTreeMap<String, PropertyModel>,
    pub methods: BTreeMap<String, CallableContract>,
}

/// Normalized contract variant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedContract {
    Callable(CallableContract),
    Interface(InterfaceContract),
}

impl NormalizedContract {
    pub fn owner(&self) -> &SymbolId {
        match self {
            Self::Callable(c) => &c.owner,
            Self::Interface(i) => &i.owner,
        }
    }

    pub fn visibility(&self) -> Visibility {
        match self {
            Self::Callable(c) => c.visibility,
            Self::Interface(i) => i.visibility,
        }
    }

    /// Compute deterministic BLAKE3 digest of the normalized contract.
    pub fn digest(&self) -> Result<String, ContractError> {
        let bytes = serde_json::to_vec(self)?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    /// Convert to domain Contract model.
    pub fn to_domain_contract(&self) -> Contract {
        match self {
            Self::Callable(c) => Contract {
                owner: c.owner.clone(),
                visibility: c.visibility,
                inputs: c
                    .parameters
                    .iter()
                    .map(|p| {
                        let opt = if p.is_optional { "?" } else { "" };
                        let ty = p.type_annotation.as_deref().unwrap_or("any");
                        format!("{}{}: {}", p.name, opt, ty)
                    })
                    .collect(),
                output: c.return_type.clone(),
                effects: Vec::new(),
            },
            Self::Interface(i) => Contract {
                owner: i.owner.clone(),
                visibility: i.visibility,
                inputs: i
                    .properties
                    .values()
                    .map(|p| {
                        let opt = if p.is_optional { "?" } else { "" };
                        let ty = p.type_annotation.as_deref().unwrap_or("any");
                        format!("{}{}: {}", p.name, opt, ty)
                    })
                    .collect(),
                output: None,
                effects: Vec::new(),
            },
        }
    }
}

/// Evaluation result comparing two versions of a contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractCompatibilityResult {
    pub is_compatible: bool,
    pub is_breaking: bool,
    pub reasons: Vec<String>,
}

/// Deterministic compatibility checker for contracts.
#[derive(Debug, Default)]
pub struct ContractCompatibilityChecker;

impl ContractCompatibilityChecker {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates backward compatibility between `before` and `after` contracts.
    pub fn check(
        &self,
        before: &NormalizedContract,
        after: &NormalizedContract,
    ) -> Result<ContractCompatibilityResult, ContractError> {
        match (before, after) {
            (NormalizedContract::Callable(b), NormalizedContract::Callable(a)) => {
                Ok(self.check_callable(b, a))
            }
            (NormalizedContract::Interface(b), NormalizedContract::Interface(a)) => {
                Ok(self.check_interface(b, a))
            }
            (NormalizedContract::Callable(_), NormalizedContract::Interface(_)) => {
                Err(ContractError::KindMismatch {
                    before_kind: "callable",
                    after_kind: "interface",
                })
            }
            (NormalizedContract::Interface(_), NormalizedContract::Callable(_)) => {
                Err(ContractError::KindMismatch {
                    before_kind: "interface",
                    after_kind: "callable",
                })
            }
        }
    }

    fn check_callable(
        &self,
        before: &CallableContract,
        after: &CallableContract,
    ) -> ContractCompatibilityResult {
        let mut reasons = Vec::new();

        // 1. Visibility check: Reducing visibility is always breaking
        if is_visibility_reduced(before.visibility, after.visibility) {
            reasons.push(format!(
                "Visibility reduced from {:?} to {:?}",
                before.visibility, after.visibility
            ));
        }

        // 2. Return type check
        if before.return_type != after.return_type {
            reasons.push(format!(
                "Return type modified from {:?} to {:?}",
                before.return_type, after.return_type
            ));
        }

        // 3. Parameters check
        let before_map: BTreeMap<&str, &ParameterModel> = before
            .parameters
            .iter()
            .map(|p| (p.name.as_str(), p))
            .collect();
        let after_map: BTreeMap<&str, &ParameterModel> = after
            .parameters
            .iter()
            .map(|p| (p.name.as_str(), p))
            .collect();

        // Detect removed parameters
        for &p_name in before_map.keys() {
            if !after_map.contains_key(p_name) {
                reasons.push(format!("Parameter '{}' removed", p_name));
            }
        }

        // Detect added required parameters
        for (&p_name, after_param) in &after_map {
            if !before_map.contains_key(p_name)
                && !after_param.is_optional
                && !after_param.has_default
            {
                reasons.push(format!("Required parameter '{}' added", p_name));
            }
        }

        // Detect parameter type alterations
        for (&p_name, before_param) in &before_map {
            if let Some(after_param) = after_map.get(p_name)
                && before_param.type_annotation != after_param.type_annotation
            {
                reasons.push(format!(
                    "Parameter '{}' type changed from {:?} to {:?}",
                    p_name, before_param.type_annotation, after_param.type_annotation
                ));
            }
        }

        reasons.sort();
        let is_breaking = !reasons.is_empty();
        ContractCompatibilityResult {
            is_compatible: !is_breaking,
            is_breaking,
            reasons,
        }
    }

    fn check_interface(
        &self,
        before: &InterfaceContract,
        after: &InterfaceContract,
    ) -> ContractCompatibilityResult {
        let mut reasons = Vec::new();

        // 1. Visibility reduction
        if is_visibility_reduced(before.visibility, after.visibility) {
            reasons.push(format!(
                "Visibility reduced from {:?} to {:?}",
                before.visibility, after.visibility
            ));
        }

        // 2. Property checks
        for prop_name in before.properties.keys() {
            if !after.properties.contains_key(prop_name) {
                reasons.push(format!("Property '{}' removed from interface", prop_name));
            }
        }

        for (prop_name, after_prop) in &after.properties {
            if !before.properties.contains_key(prop_name) && !after_prop.is_optional {
                reasons.push(format!(
                    "Required property '{}' added to interface",
                    prop_name
                ));
            }
        }

        for (prop_name, before_prop) in &before.properties {
            if let Some(after_prop) = after.properties.get(prop_name) {
                if before_prop.type_annotation != after_prop.type_annotation {
                    reasons.push(format!(
                        "Property '{}' type changed from {:?} to {:?}",
                        prop_name, before_prop.type_annotation, after_prop.type_annotation
                    ));
                }
                if !before_prop.is_readonly && after_prop.is_readonly {
                    reasons.push(format!("Property '{}' became readonly", prop_name));
                }
            }
        }

        // 3. Method checks
        for (method_name, before_method) in &before.methods {
            match after.methods.get(method_name) {
                Some(after_method) => {
                    let sub_res = self.check_callable(before_method, after_method);
                    for sub_reason in sub_res.reasons {
                        reasons.push(format!("Method '{}': {}", method_name, sub_reason));
                    }
                }
                None => {
                    reasons.push(format!("Method '{}' removed from interface", method_name));
                }
            }
        }

        reasons.sort();
        let is_breaking = !reasons.is_empty();
        ContractCompatibilityResult {
            is_compatible: !is_breaking,
            is_breaking,
            reasons,
        }
    }
}

fn is_visibility_reduced(before: Visibility, after: Visibility) -> bool {
    let rank = |v: Visibility| match v {
        Visibility::Public => 4,
        Visibility::Protected => 3,
        Visibility::Internal => 2,
        Visibility::Private => 1,
    };
    rank(after) < rank(before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_callable_compatible_optional_param() {
        let before = NormalizedContract::Callable(CallableContract {
            owner: SymbolId::new("calc::add"),
            name: "add".to_string(),
            visibility: Visibility::Public,
            parameters: vec![
                ParameterModel::new("a").with_type("number"),
                ParameterModel::new("b").with_type("number"),
            ],
            return_type: Some("number".to_string()),
            type_parameters: Vec::new(),
            is_async: false,
        });

        // Adding an optional parameter 'c' is compatible
        let after = NormalizedContract::Callable(CallableContract {
            owner: SymbolId::new("calc::add"),
            name: "add".to_string(),
            visibility: Visibility::Public,
            parameters: vec![
                ParameterModel::new("a").with_type("number"),
                ParameterModel::new("b").with_type("number"),
                ParameterModel::new("c").with_type("number").optional(),
            ],
            return_type: Some("number".to_string()),
            type_parameters: Vec::new(),
            is_async: false,
        });

        let checker = ContractCompatibilityChecker::new();
        let res = checker.check(&before, &after).unwrap();
        assert!(res.is_compatible);
        assert!(!res.is_breaking);
        assert!(res.reasons.is_empty());
    }

    #[test]
    fn test_callable_breaking_required_param() {
        let before = NormalizedContract::Callable(CallableContract {
            owner: SymbolId::new("calc::add"),
            name: "add".to_string(),
            visibility: Visibility::Public,
            parameters: vec![ParameterModel::new("a").with_type("number")],
            return_type: Some("number".to_string()),
            type_parameters: Vec::new(),
            is_async: false,
        });

        // Adding a required parameter 'b' is breaking
        let after = NormalizedContract::Callable(CallableContract {
            owner: SymbolId::new("calc::add"),
            name: "add".to_string(),
            visibility: Visibility::Public,
            parameters: vec![
                ParameterModel::new("a").with_type("number"),
                ParameterModel::new("b").with_type("number"),
            ],
            return_type: Some("number".to_string()),
            type_parameters: Vec::new(),
            is_async: false,
        });

        let checker = ContractCompatibilityChecker::new();
        let res = checker.check(&before, &after).unwrap();
        assert!(!res.is_compatible);
        assert!(res.is_breaking);
        assert_eq!(res.reasons, vec!["Required parameter 'b' added"]);
    }

    #[test]
    fn test_interface_breaking_removed_property() {
        let mut b_props = BTreeMap::new();
        b_props.insert(
            "id".to_string(),
            PropertyModel::new("id").with_type("string"),
        );
        b_props.insert(
            "name".to_string(),
            PropertyModel::new("name").with_type("string"),
        );

        let before = NormalizedContract::Interface(InterfaceContract {
            owner: SymbolId::new("models::User"),
            name: "User".to_string(),
            visibility: Visibility::Public,
            properties: b_props,
            methods: BTreeMap::new(),
        });

        // After drops 'name'
        let mut a_props = BTreeMap::new();
        a_props.insert(
            "id".to_string(),
            PropertyModel::new("id").with_type("string"),
        );

        let after = NormalizedContract::Interface(InterfaceContract {
            owner: SymbolId::new("models::User"),
            name: "User".to_string(),
            visibility: Visibility::Public,
            properties: a_props,
            methods: BTreeMap::new(),
        });

        let checker = ContractCompatibilityChecker::new();
        let res = checker.check(&before, &after).unwrap();
        assert!(!res.is_compatible);
        assert!(res.is_breaking);
        assert_eq!(res.reasons, vec!["Property 'name' removed from interface"]);
    }
}
