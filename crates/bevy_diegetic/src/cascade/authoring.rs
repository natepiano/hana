use bevy::prelude::Reflect;

/// Authored cascade value stored on panels, elements, and styles.
///
/// `Inherit` means "do not author a local value; resolve from the parent or
/// the global `CascadeDefault<T>`." `Override(value)` means this node authors a
/// local value that wins over its inherited value.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Reflect)]
pub enum Cascade<T> {
    /// Inherit the value from the cascade.
    #[default]
    Inherit,
    /// Override the inherited value with this local value.
    Override(T),
}

impl<T> Cascade<T> {
    /// Returns `true` when this value inherits from the cascade.
    #[must_use]
    pub const fn is_inherit(&self) -> bool { matches!(self, Self::Inherit) }

    /// Returns `true` when this value authors a local override.
    #[must_use]
    pub const fn is_override(&self) -> bool { matches!(self, Self::Override(_)) }

    /// Borrows the override value while preserving the cascade state.
    #[must_use]
    pub const fn as_ref(&self) -> Cascade<&T> {
        match self {
            Self::Inherit => Cascade::Inherit,
            Self::Override(value) => Cascade::Override(value),
        }
    }

    /// Returns the override value by reference, if this value authors one.
    #[must_use]
    pub const fn as_override(&self) -> Option<&T> {
        match self {
            Self::Inherit => None,
            Self::Override(value) => Some(value),
        }
    }

    /// Applies `f` to the override value or returns `default` when inherited.
    #[must_use]
    pub fn map_or<U>(self, default: U, f: impl FnOnce(T) -> U) -> U {
        match self {
            Self::Inherit => default,
            Self::Override(value) => f(value),
        }
    }

    /// Applies `f` to the override value or calls `default` when inherited.
    #[must_use]
    pub fn map_or_else<U>(self, default: impl FnOnce() -> U, f: impl FnOnce(T) -> U) -> U {
        match self {
            Self::Inherit => default(),
            Self::Override(value) => f(value),
        }
    }

    /// Applies `f` to the override value while preserving inheritance.
    #[must_use]
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Cascade<U> {
        match self {
            Self::Inherit => Cascade::Inherit,
            Self::Override(value) => Cascade::Override(f(value)),
        }
    }
}

impl<T: Copy> Cascade<T> {
    /// Copies the override value, if this value authors one.
    #[must_use]
    pub const fn copied(self) -> Option<T> {
        match self {
            Self::Inherit => None,
            Self::Override(value) => Some(value),
        }
    }

    /// Returns the local override or the supplied inherited value.
    #[must_use]
    pub const fn resolve_or(self, inherited: T) -> T {
        match self {
            Self::Inherit => inherited,
            Self::Override(value) => value,
        }
    }
}

impl<T: Clone> Cascade<&T> {
    /// Clones the borrowed override value while preserving inheritance.
    #[must_use]
    pub fn cloned(self) -> Cascade<T> {
        match self {
            Self::Inherit => Cascade::Inherit,
            Self::Override(value) => Cascade::Override(value.clone()),
        }
    }
}

impl<T> From<Option<T>> for Cascade<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Override(value),
            None => Self::Inherit,
        }
    }
}

impl<T> From<Cascade<T>> for Option<T> {
    fn from(value: Cascade<T>) -> Self {
        match value {
            Cascade::Inherit => None,
            Cascade::Override(value) => Some(value),
        }
    }
}
