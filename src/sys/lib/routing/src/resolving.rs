// Copyright 2022 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use {
    crate::{
        component_instance::{ComponentInstanceInterface, ExtendedInstanceInterface},
        error::ComponentInstanceError,
    },
    anyhow::Error,
    clonable_error::ClonableError,
    fidl_fuchsia_component_resolution as fresolution, fidl_fuchsia_io as fio,
    fuchsia_zircon_status as zx,
    lazy_static::lazy_static,
    once_cell::sync::OnceCell,
    std::convert::TryFrom,
    std::sync::Arc,
    thiserror::Error,
    url::Url,
};

lazy_static! {
    /// A default base URL from which to parse relative component URL
    /// components.
    static ref A_BASE_URL: Url = Url::parse("relative:///").unwrap();
}

/// The response returned from a Resolver. This struct is derived from the FIDL
/// [`fuchsia.component.resolution.Component`][fidl_fuchsia_component_resolution::Component]
/// table, except that the opaque binary ComponentDecl has been deserialized and validated.
#[derive(Debug)]
pub struct ResolvedComponent {
    /// A string indicating which resolver resolved this component (for log
    /// messages and debugging only).
    pub resolved_by: String,
    /// The url used to resolve this component.
    pub resolved_url: String,
    /// The package context, from the component resolution context returned by
    /// the resolver.
    pub context_to_resolve_children: Option<ComponentResolutionContext>,
    pub decl: cm_rust::ComponentDecl,
    pub package: Option<ResolvedPackage>,
    pub config_values: Option<cm_rust::ValuesData>,
}

/// The response returned from a Resolver. This struct is derived from the FIDL
/// [`fuchsia.component.resolution.Package`][fidl_fuchsia_component_resolution::Package]
/// table.
#[derive(Debug)]
pub struct ResolvedPackage {
    /// The package url.
    pub url: String,
    /// The package directory client proxy.
    pub directory: fidl::endpoints::ClientEnd<fio::DirectoryMarker>,
}

impl TryFrom<fresolution::Package> for ResolvedPackage {
    type Error = ResolverError;

    fn try_from(package: fresolution::Package) -> Result<Self, Self::Error> {
        Ok(ResolvedPackage {
            url: package.url.ok_or(ResolverError::PackageUrlMissing)?,
            directory: package.directory.ok_or(ResolverError::PackageDirectoryMissing)?,
        })
    }
}

/// When resolving a component, the resolver returns a PackageContext, which it
/// may use to resolve relative path URLs. The value is resolver-specific, so
/// a resolver that doesn't support relative path resolution can return any
/// value (typically an empty `Vec`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentResolutionContext(pub Vec<u8>);

impl ComponentResolutionContext {
    /// Converts this ComponentResolutionContext into a byte vector, which can be passed
    /// to a `Resolver::ResolveWithContext()` request with a relative component
    /// URL.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// Returns a reference to the underlying byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for ComponentResolutionContext {
    fn from(context: Vec<u8>) -> ComponentResolutionContext {
        ComponentResolutionContext(context)
    }
}

impl From<&[u8]> for ComponentResolutionContext {
    fn from(context: &[u8]) -> ComponentResolutionContext {
        ComponentResolutionContext(Vec::from(context))
    }
}

impl From<ComponentResolutionContext> for Vec<u8> {
    fn from(context: ComponentResolutionContext) -> Vec<u8> {
        context.into_bytes()
    }
}

impl From<&ComponentResolutionContext> for Vec<u8> {
    fn from(context: &ComponentResolutionContext) -> Vec<u8> {
        context.clone().into_bytes()
    }
}

impl<'a> From<&'a ComponentResolutionContext> for &'a [u8] {
    fn from(context: &'a ComponentResolutionContext) -> &'a [u8] {
        context.as_bytes()
    }
}

/// Provides the `ComponentAddress` and context for resolving a child or
/// descendent component.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedAncestorComponent {
    /// The component address, needed for relative path URLs (to get the
    /// scheme used to find the required `Resolver`), or for relative resource
    /// URLs (which will clone the parent's address, but replace the resource).
    pub address: ComponentAddress,
    /// The component's resolution_context, required for resolving descendents
    /// using a relative path component URLs.
    pub context_to_resolve_children: Option<ComponentResolutionContext>,
}

impl ResolvedAncestorComponent {
    /// Creates a `ResolvedAncestorComponent` from one of its child components.
    pub async fn direct_parent_of<C: ComponentInstanceInterface>(
        component: &Arc<C>,
    ) -> Result<Self, ResolverError> {
        let parent_component = get_parent(component).await?;
        let resolved_parent = parent_component.lock_resolved_state().await?;
        Ok(Self {
            address: resolved_parent.address(),
            context_to_resolve_children: resolved_parent.context_to_resolve_children(),
        })
    }

    /// Creates a `ResolvedAncestorComponent` from one of its child components.
    pub async fn first_packaged_ancestor_of<C: ComponentInstanceInterface>(
        component: &Arc<C>,
    ) -> Result<Self, ResolverError> {
        let mut parent_component = get_parent(component).await?;
        loop {
            // Loop until the parent has a valid context_to_resolve_children,
            // or an error getting the next parent, or its resolved state.
            {
                let resolved_parent = parent_component.lock_resolved_state().await?;
                let address = resolved_parent.address();
                // TODO(fxbug.dev/102211): change this test to something more
                // explicit, that is, return the parent's address and context if
                // the component address is a packaged component (determined in
                // some way).
                //
                // The issue being addressed here is, when resolving a relative
                // subpackaged component URL, component manager MUST resolve the
                // component using a "resolution context" _AND_ the resolver
                // that provided that context. Typically these are provided by
                // the parent component, but in the case of a RealmBuilder child
                // component, its parent is the built "realm" (which was
                // resolved by the realm_builder_resolver, using URL scheme
                // "realm-builder://"). The child component's URL is supposed to
                // be relative to the test component (the parent of the realm),
                // which was probably resolved by the full-resolver (scheme
                // "fuchsia-pkg://"). Knowing this expected topology, we can
                // skip "realm-builder" components when searching for the
                // required ancestor's URL scheme (to get the right resolver)
                // and context. This is a brittle workaround that will be
                // replaced.
                //
                // Some alternatives are under discussion, but the leading
                // candidate, for now, is to allow a resolver to return a flag
                // (with the resolved Component; perhaps `is_packaged()`) to
                // indicate that descendents should (if true) use this component
                // to get scheme and context for resolving relative path URLs
                // (for example, subpackages). If false, get the parent's parent
                // and so on.
                if address.scheme() != "realm-builder" {
                    return Ok(Self {
                        address,
                        context_to_resolve_children: resolved_parent.context_to_resolve_children(),
                    });
                }
            }
            parent_component = get_parent(&parent_component).await?;
        }
    }
}

async fn get_parent<C: ComponentInstanceInterface>(
    component: &Arc<C>,
) -> Result<Arc<C>, ResolverError> {
    if let ExtendedInstanceInterface::Component(parent_component) =
        component.try_get_parent().map_err(|err| {
            ResolverError::no_parent_context(anyhow::format_err!(
                "Component {} ({}) has no parent for context: {:?}.",
                component.abs_moniker(),
                component.url(),
                err,
            ))
        })?
    {
        Ok(parent_component.clone())
    } else {
        Err(ResolverError::no_parent_context(anyhow::format_err!(
            "Component {} ({}) has no parent for context.",
            component.abs_moniker(),
            component.url(),
        )))
    }
}

/// Indicates the kind of `ComponentAddress`, and holds `ComponentAddress`
/// properties specific to its kind. Note that there is no kind for a relative
/// resource component URL (a URL that only contains a resource fragment, such
/// as `#meta/comp.cm`) because `ComponentAddress::from()` will translate a
/// resource fragment component URL into one of the fully-resolvable
/// `ComponentAddressKind`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentAddressKind {
    /// A fully-qualified component URL, including scheme and host; for example,
    /// "fuchsia-pkg://fuchsia.com/some_package#meta/my_component.cm". Host may
    /// be empty. The query string (excluding the question mark (`?`) prefix)
    /// is optional, and only an absolute component URL may include a query.
    /// `ComponentAddress` does not constrain the value of the query string.
    /// They may be invalid for some resolvers. Some resolvers may support a
    /// query string of the form `hash=<hex-package-merkle>`.
    Absolute { host: String, some_query: Option<String> },

    /// A relative Component URL, starting with the package path; for example a
    /// subpackage relative URL such as "needed_package#meta/dep_component.cm".
    RelativePath {
        /// An opaque value (from the perspective of component resolution)
        /// required by the resolver when resolving a relative package path.
        /// For a given child component, this property is populated from a
        /// parent component's `resolution_context`, as returned by the parent
        /// component's resolver.
        context: ComponentResolutionContext,
    },
}

#[derive(Debug, Clone, Eq)]
pub struct ComponentAddress {
    /// The kind of `ComponentAddress` (`Absolute` or `RelativePath`) with
    /// kind-specific additional properties.
    kind: ComponentAddressKind,

    /// The scheme of an ancestor component's URL, used to identify the
    /// `Resolver` in a `ResolverRegistry`.
    scheme: String,

    /// The path part of the component URL. For `RelativePath`, this path MUST
    /// NOT start with a slash (`/`). For subpackages, the path MUST contain
    /// exactly one path segment (no slashes).
    path: String,

    /// The URI fragment used to identify a resource in a package. For
    /// components, this is the component manifest path. For component URLs, the
    /// path MUST NOT start with a pound sign (`#`) or slash (`/`).
    some_resource: Option<String>,

    /// The given URL used to compute the ComponentAddress, which may have
    /// required additional information from its parent component.
    some_original_url: Option<String>,

    /// Holds a resolver-ready computed URL string.
    url: OnceCell<String>,
}

/// Ignore `some_original_url` and `url` when comparing `ComponentAddress`
/// instances.
impl PartialEq for ComponentAddress {
    fn eq(&self, other: &Self) -> bool {
        let ComponentAddress { kind, scheme, path, some_resource, some_original_url: _, url: _ } =
            other;
        &self.kind == kind
            && &self.scheme == scheme
            && &self.path == path
            && &self.some_resource == some_resource
    }
}

impl ComponentAddress {
    /// Creates a new `ComponentAddress` of the `Absolute` kind.
    pub fn new_absolute(
        scheme: &str,
        host: &str,
        path: &str,
        some_query: Option<&str>,
        some_resource: Option<&str>,
    ) -> Self {
        Self {
            kind: ComponentAddressKind::Absolute {
                host: host.to_owned(),
                some_query: some_query.map(str::to_string),
            },
            scheme: scheme.to_owned(),
            path: path.to_owned(),
            some_resource: some_resource.map(str::to_string),
            some_original_url: None,
            url: OnceCell::new(),
        }
    }

    /// Creates a new `ComponentAddress` of the `RelativePath` kind.
    pub fn new_relative_path(
        path: &str,
        some_resource: Option<&str>,
        scheme: &str,
        context: ComponentResolutionContext,
    ) -> Self {
        Self {
            kind: ComponentAddressKind::RelativePath { context },
            scheme: scheme.to_owned(),
            path: path.to_owned(),
            some_resource: some_resource.map(str::to_string),
            some_original_url: None,
            url: OnceCell::new(),
        }
    }

    /// Parse the given absolute `component_url` and create a `ComponentAddress`
    /// with kind `Absolute`.
    pub fn from_absolute_url(component_url: &str) -> Result<Self, ResolverError> {
        let url = match Url::parse(component_url) {
            Ok(url) => url,
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                return Err(ResolverError::RelativeUrlNotExpected(component_url.to_string()))
            }
            Err(err) => return Err(ResolverError::malformed_url(err)),
        };
        let path = &url.path()[1..]; // skip leading "/"
        let host = url.host_str().ok_or_else(|| {
            ResolverError::malformed_url(anyhow::format_err!(
                "Parsed `host` was invalid (`None`) from URL '{}'.",
                component_url
            ))
        })?;
        let mut address = Self::new_absolute(url.scheme(), host, path, url.query(), url.fragment());
        address.some_original_url = Some(component_url.to_owned());
        Ok(address)
    }

    /// Parse the given `component_url` to determine if it is an absolute URL,
    /// a relative subpackage URL, or a relative resource URL, and return the
    /// corresponding `ComponentAddress` enum variant and value. If the URL is
    /// relative, use the component instance to get the required resolution
    /// context from the component's parent.
    pub async fn from<C: ComponentInstanceInterface>(
        component_url: &str,
        component: &Arc<C>,
    ) -> Result<Self, ResolverError> {
        let result = Self::from_absolute_url(component_url);
        if !matches!(result, Err(ResolverError::RelativeUrlNotExpected(_))) {
            return result;
        }
        let url = parse_relative_url(component_url)?;
        let path = &url.path()[1..]; // skip leading "/"
        if url.fragment().is_none() && path.is_empty() {
            return Err(ResolverError::malformed_url(anyhow::format_err!("{}", component_url)));
        }
        if url.query().is_some() {
            return Err(ResolverError::malformed_url(anyhow::format_err!(
                "Query strings are not allowed in relative component URLs: {}",
                component_url
            )));
        }
        let mut address = if path.is_empty() {
            // The `component_url` had only a fragment, so the new address will
            // be the same as its parent (for example, the same package), except
            // for its resource.
            let resolved_parent = ResolvedAncestorComponent::direct_parent_of(component).await?;
            resolved_parent.address.clone_with_new_resource(url.fragment())
        } else {
            // The `component_url` starts with a relative path (for example, a
            // subpackage name). Create a `RelativePath` address, and resolve it
            // using the `context_to_resolve_children`, from this component's
            // parent, or the first ancestor that is from a "package". (Note
            // that Realm Builder realms are synthesized, and not from a
            // package. A test component using Realm Builder will build a realm
            // and may add child components using subpackage references. Those
            // child components should get resolved using the context of the
            // test package, not the intermediate realm created via
            // RealmBuilder.)
            let resolved_ancestor =
                ResolvedAncestorComponent::first_packaged_ancestor_of(component).await?;
            let scheme = resolved_ancestor.address.scheme();
            let context = resolved_ancestor.context_to_resolve_children.clone().ok_or_else(|| {
                    ResolverError::RelativeUrlMissingContext(format!(
                        "Relative path component URL '{}' cannot be resolved because its ancestor did not provide a resolution context. The ancestor's component address is {:?}.",
                         component_url, resolved_ancestor.address
                    ))
                })?;
            Self::new_relative_path(path, url.fragment(), scheme, context)
        };
        address.some_original_url = Some(component_url.to_owned());
        Ok(address)
    }

    /// Creates a new `ComponentAddress` from `self` by replacing only the
    /// component URL resource.
    pub fn clone_with_new_resource(&self, some_resource: Option<&str>) -> Self {
        Self {
            kind: self.kind.clone(),
            scheme: self.scheme.clone(),
            path: self.path.clone(),
            some_resource: some_resource.map(str::to_string),
            some_original_url: None,
            url: OnceCell::new(),
        }
    }

    /// Returns the variant, which is `Absolute` or `RelativePath`.
    pub fn kind(&self) -> &ComponentAddressKind {
        &self.kind
    }

    /// True if the `kind()` is `Absolute`.
    pub fn is_absolute(&self) -> bool {
        matches!(self.kind, ComponentAddressKind::Absolute { .. })
    }

    /// True if the `kind()` is `RelativePath`.
    pub fn is_relative_path(&self) -> bool {
        matches!(self.kind, ComponentAddressKind::RelativePath { .. })
    }

    /// Returns the optional host value for an `Absolute` component URL.
    ///
    /// Panics if called for a `RelativePath` component address.
    pub fn host(&self) -> &str {
        if let ComponentAddressKind::Absolute { host, .. } = &self.kind {
            host
        } else {
            panic!("host() is only valid for `ComponentAddressKind::Absolute");
        }
    }

    /// Returns the `ComponentResolutionContext` value required to resolve for a
    /// `RelativePath` component URL.
    ///
    /// Panics if called for an `Absolute` component address.
    pub fn context(&self) -> &ComponentResolutionContext {
        if let ComponentAddressKind::RelativePath { context } = &self.kind {
            &context
        } else {
            panic!("host() is only valid for `ComponentAddressKind::Absolute");
        }
    }

    /// Returns the URL scheme either provided for an `Absolute` URL or derived
    /// from the component's parent. The scheme is used to look up a registered
    /// resolver, when resolving the component.
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// Returns the URL path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the optional query value for an `Absolute` component URL.
    /// Always returns `None` for `Relative` component URLs.
    pub fn query(&self) -> Option<&str> {
        if let ComponentAddressKind::Absolute { some_query, .. } = &self.kind {
            some_query.as_deref()
        } else {
            None
        }
    }

    /// Returns the optional component resource, from the URL fragment.
    pub fn resource(&self) -> Option<&str> {
        self.some_resource.as_deref()
    }

    /// Returns the original URL, if this `ComponentAddress` was created by
    /// calling `ComponentAddress::from(<original_url>, ...)`.
    pub fn original_url(&self) -> Option<&str> {
        self.some_original_url.as_deref()
    }

    /// Returns the resolver-ready URL string and, if it is a `RelativePath`,
    /// `Some(context)`, or `None` for an `Absolute` address.
    pub fn url(&self) -> &str {
        let url = self.url.get_or_init(|| match &self.kind {
            ComponentAddressKind::Absolute { host, some_query } => {
                let path = if self.path.is_empty() && !host.is_empty() {
                    "".to_string()
                } else {
                    format!("/{}", self.path)
                };
                format!(
                    "{}://{}{}{}{}",
                    self.scheme,
                    host,
                    path,
                    if let Some(query) = some_query {
                        format!("?{}", query)
                    } else {
                        "".to_string()
                    },
                    if let Some(resource) = &self.some_resource {
                        format!("#{}", resource)
                    } else {
                        "".to_string()
                    },
                )
            }
            ComponentAddressKind::RelativePath { .. } => {
                if let Some(resource) = &self.some_resource {
                    format!("{}#{}", self.path, resource)
                } else {
                    format!("{}", self.path)
                }
            }
        });
        url
    }

    /// Returns the `url()` and `Some(context)` for resolving the URL,
    /// if the kind is `RelativePath` (or `None` if `Absolute`).
    pub fn to_url_and_context(&self) -> (&str, Option<&ComponentResolutionContext>) {
        let some_context = match &self.kind {
            ComponentAddressKind::Absolute { .. } => None,
            ComponentAddressKind::RelativePath { context } => Some(context),
        };
        (self.url(), some_context)
    }
}

fn parse_relative_url(component_url: &str) -> Result<Url, ResolverError> {
    match Url::parse(component_url) {
        Ok(_) => Err(ResolverError::malformed_url(anyhow::format_err!(
            "Error parsing a relative URL given absolute URL '{}'.",
            component_url,
        ))),
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            A_BASE_URL.join(component_url).map_err(|err| {
                ResolverError::malformed_url(anyhow::format_err!(
                    "Error parsing a relative component URL '{}': {:?}.",
                    component_url,
                    err
                ))
            })
        }
        Err(err) => Err(ResolverError::malformed_url(anyhow::format_err!(
            "Unexpected error while parsing a component URL '{}': {:?}.",
            component_url,
            err,
        ))),
    }
}

/// Errors produced by built-in `Resolver`s and `resolving` APIs.
#[derive(Debug, Error, Clone)]
pub enum ResolverError {
    #[error("an unexpected error occurred: {0}")]
    Internal(#[source] ClonableError),
    #[error("an IO error occurred: {0}")]
    Io(#[source] ClonableError),
    #[error("component manifest not found: {0}")]
    ManifestNotFound(#[source] ClonableError),
    #[error("package not found: {0}")]
    PackageNotFound(#[source] ClonableError),
    #[error("component manifest invalid: {0}")]
    ManifestInvalid(#[source] ClonableError),
    #[error("config values file invalid: {0}")]
    ConfigValuesInvalid(#[source] ClonableError),
    #[error("failed to read manifest: {0}")]
    ManifestIo(zx::Status),
    #[error("failed to read config values: {0}")]
    ConfigValuesIo(zx::Status),
    #[error("Model not available")]
    ModelNotAvailable,
    #[error("scheme not registered")]
    SchemeNotRegistered,
    #[error("malformed url: {0}")]
    MalformedUrl(#[source] ClonableError),
    #[error("relative url requires a parent component with resolution context: {0}")]
    NoParentContext(#[source] ClonableError),
    #[error("package URL missing")]
    PackageUrlMissing,
    #[error("package directory handle missing")]
    PackageDirectoryMissing,
    #[error("url missing resource")]
    UrlMissingResource,
    #[error("a relative URL was not expected: {0}")]
    RelativeUrlNotExpected(String),
    #[error("failed to route resolver capability: {0}")]
    RoutingError(#[source] ClonableError),
    #[error("a resolver resolved a component but did not return its required context")]
    ResolveMustReturnContext,
    #[error("a context is required to resolve relative url: {0}")]
    RelativeUrlMissingContext(String),
    #[error("this component resolver does not resolve relative path component URLs: {0}")]
    UnexpectedRelativePath(String),
    #[error("error creating a resolution context: {0}")]
    CreatingContext(String),
    #[error("error reading a resolution context: {0}")]
    ReadingContext(String),
    #[error("the remote resolver returned invalid data")]
    RemoteInvalidData,
    #[error("an error occurred sending a FIDL request to the remote resolver: {0}")]
    FidlError(#[source] ClonableError),
}

impl ResolverError {
    pub fn internal(err: impl Into<Error>) -> Self {
        Self::Internal(err.into().into())
    }

    pub fn io(err: impl Into<Error>) -> Self {
        Self::Io(err.into().into())
    }

    pub fn manifest_not_found(err: impl Into<Error>) -> Self {
        Self::ManifestNotFound(err.into().into())
    }

    pub fn package_not_found(err: impl Into<Error>) -> Self {
        Self::PackageNotFound(err.into().into())
    }

    pub fn manifest_invalid(err: impl Into<Error>) -> Self {
        Self::ManifestInvalid(err.into().into())
    }

    pub fn config_values_invalid(err: impl Into<Error>) -> Self {
        Self::ConfigValuesInvalid(err.into().into())
    }

    pub fn malformed_url(err: impl Into<Error>) -> Self {
        Self::MalformedUrl(err.into().into())
    }

    pub fn no_parent_context(err: impl Into<Error>) -> Self {
        Self::NoParentContext(err.into().into())
    }

    pub fn routing_error(err: impl Into<Error>) -> Self {
        Self::RoutingError(err.into().into())
    }

    pub fn fidl_error(err: impl Into<Error>) -> Self {
        Self::FidlError(err.into().into())
    }
}

impl From<fresolution::ResolverError> for ResolverError {
    fn from(err: fresolution::ResolverError) -> ResolverError {
        match err {
            fresolution::ResolverError::Internal => ResolverError::internal(RemoteError(err)),
            fresolution::ResolverError::Io => ResolverError::io(RemoteError(err)),
            fresolution::ResolverError::PackageNotFound
            | fresolution::ResolverError::NoSpace
            | fresolution::ResolverError::ResourceUnavailable
            | fresolution::ResolverError::NotSupported => {
                ResolverError::package_not_found(RemoteError(err))
            }
            fresolution::ResolverError::ManifestNotFound => {
                ResolverError::manifest_not_found(RemoteError(err))
            }
            fresolution::ResolverError::InvalidArgs => {
                ResolverError::malformed_url(RemoteError(err))
            }
            fresolution::ResolverError::InvalidManifest => {
                ResolverError::ManifestInvalid(anyhow::Error::from(RemoteError(err)).into())
            }
            fresolution::ResolverError::ConfigValuesNotFound => {
                ResolverError::ConfigValuesIo(zx::Status::NOT_FOUND)
            }
        }
    }
}

impl From<ComponentInstanceError> for ResolverError {
    fn from(err: ComponentInstanceError) -> ResolverError {
        use ComponentInstanceError::*;
        match &err {
            ComponentManagerInstanceUnavailable {}
            | InstanceNotFound { .. }
            | PolicyCheckerNotFound { .. }
            | ComponentIdIndexNotFound { .. }
            | ResolveFailed { .. }
            | UnresolveFailed { .. } => {
                ResolverError::Internal(ClonableError::from(anyhow::format_err!("{:?}", err)))
            }
            NoAbsoluteUrl { .. } => ResolverError::NoParentContext(ClonableError::from(
                anyhow::format_err!("{:?}", err),
            )),
            MalformedUrl { .. } => {
                ResolverError::MalformedUrl(ClonableError::from(anyhow::format_err!("{:?}", err)))
            }
        }
    }
}

#[derive(Error, Clone, Debug)]
#[error("remote resolver responded with {0:?}")]
struct RemoteError(fresolution::ResolverError);

#[cfg(test)]
mod tests {
    use {super::*, assert_matches::assert_matches, fidl::endpoints::create_endpoints};

    #[test]
    fn test_resolved_package() -> anyhow::Result<()> {
        let url = "some_url".to_string();
        let (dir_client, _) =
            create_endpoints::<fio::DirectoryMarker>().expect("failed to create Directory proxy");
        let fidl_package = fresolution::Package {
            url: Some(url.clone()),
            directory: Some(dir_client),
            ..fresolution::Package::EMPTY
        };
        let resolved_package = ResolvedPackage::try_from(fidl_package)?;
        assert_eq!(resolved_package.url, url);
        Ok(())
    }

    #[test]
    fn test_component_address() -> anyhow::Result<()> {
        let address =
            ComponentAddress::from_absolute_url("some-scheme://fuchsia.com/package#meta/comp.cm")?;
        if let ComponentAddressKind::Absolute { host, some_query } = address.kind() {
            assert_eq!(host.as_str(), "fuchsia.com");
            assert_eq!(some_query, &None);
        }
        assert!(address.is_absolute());
        assert_eq!(address.host(), "fuchsia.com");
        assert_eq!(address.scheme(), "some-scheme");
        assert_eq!(address.path(), "package");
        assert_eq!(address.query(), None);
        assert_eq!(address.resource(), Some("meta/comp.cm"));
        assert_eq!(address.original_url(), Some("some-scheme://fuchsia.com/package#meta/comp.cm"));
        assert_eq!(address.url(), "some-scheme://fuchsia.com/package#meta/comp.cm");
        assert_matches!(
            address.to_url_and_context(),
            ("some-scheme://fuchsia.com/package#meta/comp.cm", None)
        );

        let abs_address = ComponentAddress::new_absolute(
            "some-scheme",
            "fuchsia.com",
            "package",
            None,
            Some("meta/comp.cm"),
        );
        // Note that `url()` has been called on `address` but not on
        // `abs_address`, and `original_url()` differences should be ignored
        // when comparing.
        assert_eq!(abs_address, address);

        assert_eq!(abs_address, address);
        if let ComponentAddressKind::Absolute { host, .. } = abs_address.kind() {
            assert_eq!(host.as_str(), "fuchsia.com");
        }
        assert!(abs_address.is_absolute());
        assert_eq!(abs_address.host(), "fuchsia.com");
        assert_eq!(abs_address.scheme(), "some-scheme");
        assert_eq!(abs_address.path(), "package");
        assert_eq!(abs_address.query(), None);
        assert_eq!(abs_address.resource(), Some("meta/comp.cm"));
        assert_eq!(abs_address.url(), "some-scheme://fuchsia.com/package#meta/comp.cm");
        assert_matches!(
            abs_address.to_url_and_context(),
            ("some-scheme://fuchsia.com/package#meta/comp.cm", None)
        );

        let cloned_address = abs_address.clone();
        assert_eq!(abs_address, cloned_address);

        let address2 = abs_address.clone_with_new_resource(Some("meta/other_comp.cm"));
        assert_ne!(address2, abs_address);
        assert!(address2.is_absolute());
        assert_eq!(address2.resource(), Some("meta/other_comp.cm"));
        assert_eq!(address2.scheme(), "some-scheme");
        assert_eq!(address2.host(), "fuchsia.com");
        assert_eq!(address2.path(), "package");
        assert_eq!(address2.query(), None);

        let rel_address = ComponentAddress::new_relative_path(
            "subpackage",
            Some("meta/subcomp.cm"),
            "some-scheme",
            ComponentResolutionContext(vec![b'4', b'5', b'6']),
        );
        if let ComponentAddressKind::RelativePath { context } = rel_address.kind() {
            assert_eq!(context.as_bytes(), &vec![b'4', b'5', b'6']);
        }
        assert!(rel_address.is_relative_path());
        assert_eq!(rel_address.context().as_bytes(), &vec![b'4', b'5', b'6']);
        assert_eq!(rel_address.original_url(), None);
        assert_eq!(rel_address.url(), "subpackage#meta/subcomp.cm");
        assert_eq!(
            rel_address.to_url_and_context(),
            (
                "subpackage#meta/subcomp.cm",
                Some(&ComponentResolutionContext(vec![b'4', b'5', b'6']))
            )
        );

        let rel_address2 = rel_address.clone_with_new_resource(Some("meta/other_subcomp.cm"));
        assert_ne!(rel_address2, rel_address);
        assert!(rel_address2.is_relative_path());
        assert_eq!(rel_address2.context().as_bytes(), &vec![b'4', b'5', b'6']);
        assert_eq!(rel_address2.original_url(), None);
        assert_eq!(rel_address2.url(), "subpackage#meta/other_subcomp.cm");
        assert_eq!(
            rel_address2.to_url_and_context(),
            (
                "subpackage#meta/other_subcomp.cm",
                Some(&ComponentResolutionContext(vec![b'4', b'5', b'6']))
            )
        );

        let address = ComponentAddress::from_absolute_url("fuchsia-boot:///#meta/root.cm")?;
        if let ComponentAddressKind::Absolute { host, .. } = address.kind() {
            assert_eq!(host.as_str(), "");
        }
        assert!(address.is_absolute());
        assert_eq!(address.host(), "");
        assert_eq!(address.scheme(), "fuchsia-boot");
        assert_eq!(address.path(), "");
        assert_eq!(address.query(), None);
        assert_eq!(address.resource(), Some("meta/root.cm"));
        assert_eq!(address.original_url(), Some("fuchsia-boot:///#meta/root.cm"));
        assert_eq!(address.url(), "fuchsia-boot:///#meta/root.cm");
        assert_matches!(address.to_url_and_context(), ("fuchsia-boot:///#meta/root.cm", None));

        let address = ComponentAddress::from_absolute_url(
            "fuchsia-pkg://fuchsia.com/package?hash=cafe0123#meta/comp.cm",
        )?;
        if let ComponentAddressKind::Absolute { host, some_query } = address.kind() {
            assert_eq!(host.as_str(), "fuchsia.com");
            assert_eq!(some_query.as_deref(), Some("hash=cafe0123"));
        }
        assert!(address.is_absolute());
        assert_eq!(address.host(), "fuchsia.com");
        assert_eq!(address.scheme(), "fuchsia-pkg");
        assert_eq!(address.path(), "package");
        assert_eq!(address.resource(), Some("meta/comp.cm"));
        assert_eq!(address.query(), Some("hash=cafe0123"));
        assert_eq!(
            address.original_url(),
            Some("fuchsia-pkg://fuchsia.com/package?hash=cafe0123#meta/comp.cm")
        );
        assert_eq!(address.url(), "fuchsia-pkg://fuchsia.com/package?hash=cafe0123#meta/comp.cm");
        assert_matches!(
            address.to_url_and_context(),
            ("fuchsia-pkg://fuchsia.com/package?hash=cafe0123#meta/comp.cm", None)
        );

        Ok(())
    }
}
