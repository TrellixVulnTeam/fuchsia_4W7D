// Copyright 2022 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use {
    anyhow::Error,
    diagnostics_data::{Data, Logs},
    example_tester::{logs_to_str, run_test, Client, Server, TestKind},
    fidl::prelude::*,
    fidl_examples_keyvaluestore::WriteError,
    fuchsia_async as fasync,
    fuchsia_component_test::{ChildRef, RealmBuilder},
};

#[fasync::run_singlethreaded(test)]
async fn test_write_item_success() -> Result<(), Error> {
    let test_name = "test_write_item_success";
    let client = Client::new(test_name, "#meta/keyvaluestore_client.cm");
    let server = Server::new(test_name, "#meta/keyvaluestore_server.cm");

    run_test(
        fidl_examples_keyvaluestore::StoreMarker::PROTOCOL_NAME,
        TestKind::ClientAndServer { client: &client, server: &server },
        |builder: RealmBuilder, client: ChildRef| async move {
            builder.init_mutable_config_to_empty(&client).await?;
            builder.set_config_value_string_vector(&client, "write_items", ["verse_1"]).await?;
            Ok::<(RealmBuilder, ChildRef), Error>((builder, client))
        },
        |raw_logs: Vec<Data<Logs>>| {
            // Both components have started successfully.
            let all_logs = logs_to_str(&raw_logs, None);
            assert_eq!(all_logs.filter(|log| *log == "Started").count(), 2);

            // Ensure that the server received the request, logged it, and responded successfully.
            let server_logs = logs_to_str(&raw_logs, Some(vec![&server])).collect::<Vec<&str>>();
            assert_eq!(
                server_logs
                    .clone()
                    .into_iter()
                    .find(|log| *log == "WriteItem request received")
                    .is_some(),
                true
            );
            assert_eq!(
                server_logs.into_iter().find(|log| *log == "WriteItem response sent").is_some(),
                true
            );

            // Ensure that the client received the correct reply from the server.
            let client_logs = logs_to_str(&raw_logs, Some(vec![&client]));
            assert_eq!(client_logs.last().expect("no response"), "WriteItem Success");
        },
    )
    .await
}

async fn test_write_item_invalid(
    test_name: &str,
    input: &str,
    expect: WriteError,
) -> Result<(), Error> {
    let client = Client::new(test_name, "#meta/keyvaluestore_client.cm");
    let server = Server::new(test_name, "#meta/keyvaluestore_server.cm");

    run_test(
        fidl_examples_keyvaluestore::StoreMarker::PROTOCOL_NAME,
        TestKind::ClientAndServer { client: &client, server: &server },
        |builder: RealmBuilder, client: ChildRef| async move {
            builder.init_mutable_config_to_empty(&client).await?;
            builder.set_config_value_string_vector(&client, "write_items", [input]).await?;
            Ok::<(RealmBuilder, ChildRef), Error>((builder, client))
        },
        |raw_logs: Vec<Data<Logs>>| {
            // Both components have started successfully.
            let all_logs = logs_to_str(&raw_logs, None);
            assert_eq!(all_logs.filter(|log| *log == "Started").count(), 2);

            // Ensure that the server received the request, logged it, and responded successfully.
            let server_logs = logs_to_str(&raw_logs, Some(vec![&server])).collect::<Vec<&str>>();
            assert_eq!(
                server_logs
                    .clone()
                    .into_iter()
                    .find(|log| *log == "WriteItem request received")
                    .is_some(),
                true
            );
            assert_eq!(
                server_logs.into_iter().find(|log| *log == "WriteItem response sent").is_some(),
                true
            );

            // The reply should be an INVALID_KEY error.
            let client_logs = logs_to_str(&raw_logs, Some(vec![&client]));
            assert_eq!(
                client_logs.last().expect("no response"),
                format!("WriteItem Error: {}", expect.into_primitive())
            );
        },
    )
    .await
}

#[fasync::run_singlethreaded(test)]
async fn test_write_item_error_invalid_key() -> Result<(), Error> {
    test_write_item_invalid(
        "test_write_item_error_invalid_key",
        // A trailing underscore makes for an invalid key per the rules in
        // key_value_store.test.fidl, hence the odd name here.
        "error_invalid_key_",
        WriteError::InvalidKey,
    )
    .await
}

#[fasync::run_singlethreaded(test)]
async fn test_write_item_error_invalid_value() -> Result<(), Error> {
    test_write_item_invalid(
        "test_write_item_error_invalid_value",
        "error_invalid_value",
        WriteError::InvalidValue,
    )
    .await
}

#[fasync::run_singlethreaded(test)]
async fn test_write_item_error_already_found() -> Result<(), Error> {
    let test_name = "test_write_item_error_already_found";
    let client = Client::new(test_name, "#meta/keyvaluestore_client.cm");
    let server = Server::new(test_name, "#meta/keyvaluestore_server.cm");

    run_test(
        fidl_examples_keyvaluestore::StoreMarker::PROTOCOL_NAME,
        TestKind::ClientAndServer { client: &client, server: &server },
        |builder: RealmBuilder, client: ChildRef| async move {
            builder.init_mutable_config_to_empty(&client).await?;
            builder
                .set_config_value_string_vector(&client, "write_items", ["verse_1", "verse_1"])
                .await?;
            Ok::<(RealmBuilder, ChildRef), Error>((builder, client))
        },
        |raw_logs: Vec<Data<Logs>>| {
            // Both components have started successfully.
            let all_logs = logs_to_str(&raw_logs, None);
            assert_eq!(all_logs.filter(|log| *log == "Started").count(), 2);

            // Ensure that two requests have been completed.
            let server_logs = logs_to_str(&raw_logs, Some(vec![&server])).collect::<Vec<&str>>();
            assert_eq!(
                server_logs
                    .clone()
                    .into_iter()
                    .filter(|log| *log == "WriteItem request received")
                    .count(),
                2
            );
            assert_eq!(
                server_logs.into_iter().filter(|log| *log == "WriteItem response sent").count(),
                2
            );

            // Ensure that the first request succeed...
            let client_logs = logs_to_str(&raw_logs, Some(vec![&client])).collect::<Vec<&str>>();
            assert_eq!(
                client_logs.clone().into_iter().find(|log| *log == "WriteItem Success").is_some(),
                true
            );

            // ...but the second one failed with an ALREADY_EXISTS error.
            assert_eq!(client_logs.into_iter().last().expect("no response"), "WriteItem Error: 3");
        },
    )
    .await
}
