// Copyright 2020 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use {
    component_events::{
        events::{self, Event, EventStream},
        matcher::{EventMatcher, ExitStatusMatcher},
        sequence::{self, EventSequence},
    },
    fidl_fuchsia_sys2 as fsys,
    fuchsia_component_test::{Capability, ChildOptions, RealmBuilder, Ref, Route},
};

#[fuchsia::test]
/// Verifies that when a component has a LogSink in its namespace that the
/// component manager tries to connect to this and that if that component
/// tries to use a capability it isn't offered we see the expect log content
/// which is generated by component manager, but should be attributed to the
/// component that tried to use the capability.
///
/// Part of the test runs inside the `reader` component inside the test
/// topology. If `reader` sees the log it expects from the Archivist component
/// it exits cleanly, otherwise it crashes.
async fn verify_routing_failure_messages() {
    let builder = RealmBuilder::new().await.unwrap();
    let root =
        builder.add_child("root", "#meta/e2e-root.cm", ChildOptions::new().eager()).await.unwrap();
    builder
        .add_route(
            Route::new()
                .capability(Capability::protocol_by_name("fuchsia.logger.LogSink"))
                .capability(Capability::protocol_by_name("fuchsia.sys2.EventSource"))
                .from(Ref::parent())
                .to(&root),
        )
        .await
        .unwrap();

    let instance =
        builder.build_in_nested_component_manager("#meta/component_manager.cm").await.unwrap();
    let proxy =
        instance.root.connect_to_protocol_at_exposed_dir::<fsys::EventStream2Marker>().unwrap();
    let event_stream = EventStream::new_v2(proxy);

    let expected = EventSequence::new()
        .has_subset(
            vec![
                EventMatcher::ok()
                    .r#type(events::Stopped::TYPE)
                    .moniker("./root/routing-tests/child")
                    .stop(Some(ExitStatusMatcher::AnyCrash)),
                EventMatcher::ok()
                    .r#type(events::Stopped::TYPE)
                    .moniker(
                        "./root/routing-tests/offers-to-children-unavailable/child-for-offer-from-parent",
                    )
                    .stop(Some(ExitStatusMatcher::AnyCrash)),
                EventMatcher::ok()
                    .r#type(events::Stopped::TYPE)
                    .moniker(
                        "./root/routing-tests/offers-to-children-unavailable/child-for-offer-from-sibling",
                    ).stop(Some(ExitStatusMatcher::AnyCrash)),
                EventMatcher::ok()
                    .r#type(events::Stopped::TYPE)
                    .moniker(
                        "./root/routing-tests/offers-to-children-unavailable/child-open-unrequested",
                    )
                    .stop(Some(ExitStatusMatcher::AnyCrash)),
                EventMatcher::ok()
                    .r#type(events::Stopped::TYPE)
                    .moniker("./root/reader")
                    .stop(Some(ExitStatusMatcher::Clean)),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.diagnostics.ArchiveAccessor")
                    .moniker("./root/reader"),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.logger.LogSink")
                    .moniker("./root/reader"),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.logger.LogSink")
                    .moniker("./root/reader"),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.logger.LogSink")
                    .moniker("./root/routing-tests/child"),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.logger.LogSink")
                    .moniker(
                        "./root/routing-tests/offers-to-children-unavailable/child-for-offer-from-parent",
                    ),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.logger.LogSink")
                    .moniker(
                        "./root/routing-tests/offers-to-children-unavailable/child-for-offer-from-sibling",
                    ),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.logger.LogSink")
                    .moniker(
                        "./root/routing-tests/offers-to-children-unavailable/child-open-unrequested",
                    ),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.logger.LogSink")
                    .moniker("./root/archivist"),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.logger.LogSink")
                    .moniker("./root/archivist"),
                EventMatcher::ok()
                    .r#type(events::CapabilityRouted::TYPE)
                    .capability_name("fuchsia.sys2.EventSource")
                    .moniker("./root/archivist"),
            ],
            sequence::Ordering::Unordered,
        )
        .expect(event_stream);

    instance.start_component_tree().await.unwrap();
    expected.await.unwrap();
}
