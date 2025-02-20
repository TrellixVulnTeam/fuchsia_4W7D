// Copyright 2022 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include <fidl/serversuite/cpp/fidl.h>
#include <lib/async-loop/cpp/loop.h>
#include <lib/async-loop/default.h>
#include <lib/async/cpp/task.h>
#include <lib/async/default.h>
#include <lib/fidl/cpp/binding.h>
#include <lib/fidl/cpp/synchronous_interface_ptr.h>
#include <lib/sys/cpp/component_context.h>
#include <stdio.h>

#include <iostream>

class ClosedTargetServer : public fidl::serversuite::ClosedTarget {
 public:
  explicit ClosedTargetServer(fidl::InterfacePtr<fidl::serversuite::Reporter> reporter)
      : reporter_(std::move(reporter)) {}

  void OneWayNoPayload() override {
    std::cout << "ClosedTarget.OneWayNoPayload()" << std::endl;
    reporter_->ReceivedOneWayNoPayload();
  }

  void TwoWayNoPayload(TwoWayNoPayloadCallback callback) override {
    std::cout << "ClosedTarget.TwoWayNoPayload()" << std::endl;
    callback();
  }

  void TwoWayStructPayload(int8_t v, TwoWayStructPayloadCallback callback) override {
    std::cout << "ClosedTarget.TwoWayStructPayload()" << std::endl;
    callback(v);
  }

  void TwoWayTablePayload(::fidl::serversuite::ClosedTargetTwoWayTablePayloadRequest request,
                          TwoWayTablePayloadCallback callback) override {
    std::cout << "ClosedTarget.TwoWayTablePayload()" << std::endl;
    fidl::serversuite::ClosedTargetTwoWayTablePayloadResponse response;
    response.set_v(request.v());
    callback(std::move(response));
  }

  void TwoWayUnionPayload(::fidl::serversuite::ClosedTargetTwoWayUnionPayloadRequest request,
                          TwoWayUnionPayloadCallback callback) override {
    std::cout << "ClosedTarget.TwoWayUnionPayload()" << std::endl;
    fidl::serversuite::ClosedTargetTwoWayUnionPayloadResponse response;
    response.set_v(request.v());
    callback(std::move(response));
  }

  void TwoWayResult(::fidl::serversuite::ClosedTargetTwoWayResultRequest request,
                    TwoWayResultCallback callback) override {
    std::cout << "ClosedTarget.TwoWayResult()" << std::endl;
    switch (request.Which()) {
      case fidl::serversuite::ClosedTargetTwoWayResultRequest::kPayload:
        callback(fidl::serversuite::ClosedTarget_TwoWayResult_Result::WithResponse(
            fidl::serversuite::ClosedTarget_TwoWayResult_Response(request.payload())));
        break;
      case fidl::serversuite::ClosedTargetTwoWayResultRequest::kError:
        callback(fidl::serversuite::ClosedTarget_TwoWayResult_Result::WithErr(
            std::move(request.error())));
        break;
      case fidl::serversuite::ClosedTargetTwoWayResultRequest::Invalid:
        ZX_PANIC("unexpected invalid case");
        break;
    }
  }

  void GetHandleRights(zx::handle handle, GetHandleRightsCallback callback) override {
    zx_info_handle_basic_t info;
    ZX_ASSERT(ZX_OK ==
              handle.get_info(ZX_INFO_HANDLE_BASIC, &info, sizeof(info), nullptr, nullptr));
    callback(info.rights);
  }

  void GetSignalableEventRights(zx::event event,
                                GetSignalableEventRightsCallback callback) override {
    zx_info_handle_basic_t info;
    ZX_ASSERT(ZX_OK == event.get_info(ZX_INFO_HANDLE_BASIC, &info, sizeof(info), nullptr, nullptr));
    callback(info.rights);
  }

  void EchoAsTransferableSignalableEvent(
      zx::handle handle, EchoAsTransferableSignalableEventCallback callback) override {
    callback(zx::event(handle.release()));
  }

  void CloseWithEpitaph(int32_t epitaph_status) override {
    ZX_ASSERT(ZX_OK == binding_->Close(epitaph_status));
  }

  void ByteVectorSize(std::vector<uint8_t> vec, ByteVectorSizeCallback callback) override {
    callback(static_cast<uint32_t>(vec.size()));
  }

  void HandleVectorSize(std::vector<zx::event> vec, HandleVectorSizeCallback callback) override {
    callback(static_cast<uint32_t>(vec.size()));
  }

  void CreateNByteVector(uint32_t n, CreateNByteVectorCallback callback) override {
    std::vector<uint8_t> bytes(n);
    callback(std::move(bytes));
  }

  void CreateNHandleVector(uint32_t n, CreateNHandleVectorCallback callback) override {
    std::vector<zx::event> handles(n);
    for (auto& handle : handles) {
      ZX_ASSERT(ZX_OK == zx::event::create(0, &handle));
    }
    callback(std::move(handles));
  }

  void set_binding(fidl::Binding<fidl::serversuite::ClosedTarget>* binding) { binding_ = binding; }

 private:
  fidl::InterfacePtr<fidl::serversuite::Reporter> reporter_;
  fidl::Binding<fidl::serversuite::ClosedTarget>* binding_ = nullptr;
};

class AjarTargetServer : public fidl::serversuite::AjarTarget {
 public:
  explicit AjarTargetServer(fidl::InterfacePtr<fidl::serversuite::Reporter> reporter)
      : reporter_(std::move(reporter)) {}

 protected:
  void handle_unknown_method(uint64_t ordinal) override {
    reporter_->ReceivedUnknownMethod(ordinal, fidl::serversuite::UnknownMethodType::ONE_WAY);
  }

 private:
  fidl::InterfacePtr<fidl::serversuite::Reporter> reporter_;
};

class OpenTargetServer : public fidl::serversuite::OpenTarget {
 public:
  explicit OpenTargetServer(fidl::InterfacePtr<fidl::serversuite::Reporter> reporter)
      : reporter_(std::move(reporter)) {}

  void set_event_sender(fidl::serversuite::OpenTarget_EventSender* event_sender) {
    ZX_ASSERT_MSG(!event_sender_, "event sender already set");
    event_sender_ = event_sender;
  }

  void SendEvent(::fidl::serversuite::EventType event_type) override {
    switch (event_type) {
      case fidl::serversuite::EventType::STRICT:
        event_sender_->StrictEvent();
        return;
      case fidl::serversuite::EventType::FLEXIBLE:
        event_sender_->FlexibleEvent();
        return;
      default:
        ZX_PANIC("Unknown EventType");
    }
  }

  void StrictOneWay() override { reporter_->ReceivedStrictOneWay(); }

  void FlexibleOneWay() override { reporter_->ReceivedFlexibleOneWay(); }

  void StrictTwoWay(StrictTwoWayCallback callback) override { callback(); }

  void StrictTwoWayFields(int32_t reply_with, StrictTwoWayFieldsCallback callback) override {
    callback(reply_with);
  }

  void StrictTwoWayErr(::fidl::serversuite::OpenTargetStrictTwoWayErrRequest request,
                       StrictTwoWayErrCallback callback) override {
    if (request.is_reply_success()) {
      callback(fidl::serversuite::OpenTarget_StrictTwoWayErr_Result::WithResponse({}));
    } else if (request.is_reply_error()) {
      callback(fidl::serversuite::OpenTarget_StrictTwoWayErr_Result::WithErr(
          std::move(request.reply_error())));
    } else {
      ZX_PANIC("Unrecognized StrictTwoWayErr Request Variant");
    }
  }

  void StrictTwoWayFieldsErr(::fidl::serversuite::OpenTargetStrictTwoWayFieldsErrRequest request,
                             StrictTwoWayFieldsErrCallback callback) override {
    if (request.is_reply_success()) {
      callback(fidl::serversuite::OpenTarget_StrictTwoWayFieldsErr_Result::WithResponse(
          fidl::serversuite::OpenTarget_StrictTwoWayFieldsErr_Response(request.reply_success())));
    } else if (request.is_reply_error()) {
      callback(fidl::serversuite::OpenTarget_StrictTwoWayFieldsErr_Result::WithErr(
          std::move(request.reply_error())));
    } else {
      ZX_PANIC("Unrecognized StrictTwoWayFieldsErr Request Variant");
    }
  }

  void FlexibleTwoWay(FlexibleTwoWayCallback callback) override {
    callback(fidl::serversuite::OpenTarget_FlexibleTwoWay_Result::WithResponse({}));
  }

  void FlexibleTwoWayFields(int32_t reply_with, FlexibleTwoWayFieldsCallback callback) override {
    callback(fidl::serversuite::OpenTarget_FlexibleTwoWayFields_Result::WithResponse(
        fidl::serversuite::OpenTarget_FlexibleTwoWayFields_Response(reply_with)));
  }

  void FlexibleTwoWayErr(::fidl::serversuite::OpenTargetFlexibleTwoWayErrRequest request,
                         FlexibleTwoWayErrCallback callback) override {
    if (request.is_reply_success()) {
      callback(fidl::serversuite::OpenTarget_FlexibleTwoWayErr_Result::WithResponse({}));
    } else if (request.is_reply_error()) {
      callback(fidl::serversuite::OpenTarget_FlexibleTwoWayErr_Result::WithErr(
          std::move(request.reply_error())));
    } else {
      ZX_PANIC("Unrecognized FlexibleTwoWayErr Request Variant");
    }
  }

  void FlexibleTwoWayFieldsErr(
      ::fidl::serversuite::OpenTargetFlexibleTwoWayFieldsErrRequest request,
      FlexibleTwoWayFieldsErrCallback callback) override {
    if (request.is_reply_success()) {
      callback(fidl::serversuite::OpenTarget_FlexibleTwoWayFieldsErr_Result::WithResponse(
          fidl::serversuite::OpenTarget_FlexibleTwoWayFieldsErr_Response(request.reply_success())));
    } else if (request.is_reply_error()) {
      callback(fidl::serversuite::OpenTarget_FlexibleTwoWayFieldsErr_Result::WithErr(
          std::move(request.reply_error())));
    } else {
      ZX_PANIC("Unrecognized FlexibleTwoWayFieldsErr Request Variant");
    }
  }

 protected:
  void handle_unknown_method(uint64_t ordinal, bool method_has_response) override {
    auto unknown_method_type = method_has_response ? fidl::serversuite::UnknownMethodType::TWO_WAY
                                                   : fidl::serversuite::UnknownMethodType::ONE_WAY;
    reporter_->ReceivedUnknownMethod(ordinal, unknown_method_type);
  }

 private:
  fidl::InterfacePtr<fidl::serversuite::Reporter> reporter_;
  fidl::serversuite::OpenTarget_EventSender* event_sender_ = nullptr;
};

class RunnerServer : public fidl::serversuite::Runner {
 public:
  explicit RunnerServer(async_dispatcher_t* dispatcher) : dispatcher_(dispatcher) {}

  void IsTestEnabled(fidl::serversuite::Test test, IsTestEnabledCallback callback) override {
    switch (test) {
      case fidl::serversuite::Test::BAD_AT_REST_FLAGS_CAUSES_CLOSE:
      case fidl::serversuite::Test::BAD_DYNAMIC_FLAGS_CAUSES_CLOSE:
      case fidl::serversuite::Test::SERVER_SENDS_TOO_FEW_RIGHTS:
      case fidl::serversuite::Test::RESPONSE_EXCEEDS_BYTE_LIMIT:
      case fidl::serversuite::Test::RESPONSE_EXCEEDS_HANDLE_LIMIT:
        callback(false);
        return;
      default:
        callback(true);
        return;
    }
  }

  void Start(fidl::InterfaceHandle<fidl::serversuite::Reporter> reporter,
             fidl::serversuite::AnyTarget target, StartCallback callback) override {
    if (target.is_closed_target()) {
      closed_target_server_ = std::make_unique<ClosedTargetServer>(reporter.Bind());
      closed_target_binding_ = std::make_unique<fidl::Binding<fidl::serversuite::ClosedTarget>>(
          closed_target_server_.get());
      closed_target_server_->set_binding(closed_target_binding_.get());

      closed_target_binding_->Bind(std::move(target.closed_target()), dispatcher_);
      callback();
    } else if (target.is_ajar_target()) {
      ajar_target_server_ = std::make_unique<AjarTargetServer>(reporter.Bind());
      ajar_target_binding_ =
          std::make_unique<fidl::Binding<fidl::serversuite::AjarTarget>>(ajar_target_server_.get());
      ajar_target_binding_->Bind(std::move(target.ajar_target()), dispatcher_);
      callback();
    } else if (target.is_open_target()) {
      open_target_server_ = std::make_unique<OpenTargetServer>(reporter.Bind());
      open_target_binding_ =
          std::make_unique<fidl::Binding<fidl::serversuite::OpenTarget>>(open_target_server_.get());
      open_target_server_->set_event_sender(&open_target_binding_->events());

      open_target_binding_->Bind(std::move(target.open_target()), dispatcher_);
      callback();
    } else {
      ZX_PANIC("Unrecognized target type.");
    }
  }

  void CheckAlive(CheckAliveCallback callback) override { return callback(); }

 private:
  async_dispatcher_t* dispatcher_;
  std::unique_ptr<ClosedTargetServer> closed_target_server_;
  std::unique_ptr<fidl::Binding<fidl::serversuite::ClosedTarget>> closed_target_binding_;
  std::unique_ptr<AjarTargetServer> ajar_target_server_;
  std::unique_ptr<fidl::Binding<fidl::serversuite::AjarTarget>> ajar_target_binding_;
  std::unique_ptr<OpenTargetServer> open_target_server_;
  std::unique_ptr<fidl::Binding<fidl::serversuite::OpenTarget>> open_target_binding_;
};

int main(int argc, const char** argv) {
  std::cout << "HLCPP server: main" << std::endl;
  async::Loop loop(&kAsyncLoopConfigAttachToCurrentThread);

  RunnerServer server(loop.dispatcher());
  fidl::Binding<fidl::serversuite::Runner> binding(&server);
  fidl::InterfaceRequestHandler<fidl::serversuite::Runner> handler =
      [&](fidl::InterfaceRequest<fidl::serversuite::Runner> server_end) {
        binding.Bind(std::move(server_end));
      };
  auto context = sys::ComponentContext::CreateAndServeOutgoingDirectory();
  context->outgoing()->AddPublicService(std::move(handler));

  std::cout << "HLCPP server: ready!" << std::endl;
  return loop.Run();
}
