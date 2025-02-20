// Copyright 2022 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include "src/media/audio/services/mixer/fidl/consumer_node.h"

#include <lib/syslog/cpp/macros.h>

#include "src/media/audio/services/common/logging.h"

namespace media_audio {

// static
std::shared_ptr<ConsumerNode> ConsumerNode::Create(Args args) {
  struct WithPublicCtor : public ConsumerNode {
   public:
    explicit WithPublicCtor(std::string_view name, PipelineDirection pipeline_direction,
                            ConsumerStagePtr pipeline_stage, const Format& format,
                            std::shared_ptr<CommandQueue> command_queue,
                            std::shared_ptr<GraphMixThread> mix_thread)
        : ConsumerNode(name, pipeline_direction, std::move(pipeline_stage), format,
                       std::move(command_queue), std::move(mix_thread)) {}
  };

  auto command_queue = std::make_shared<CommandQueue>();
  auto pipeline_stage = std::make_shared<ConsumerStage>(ConsumerStage::Args{
      .name = args.name,
      .pipeline_direction = args.pipeline_direction,
      // TODO(fxbug.dev/87651): also presentation_delay
      .format = args.format,
      .reference_clock = std::move(args.reference_clock),
      .command_queue = command_queue,
      .writer = std::move(args.writer),
  });
  pipeline_stage->set_thread(args.thread->pipeline_thread());
  args.thread->AddConsumer(pipeline_stage);

  auto node = std::make_shared<WithPublicCtor>(args.name, args.pipeline_direction,
                                               std::move(pipeline_stage), args.format,
                                               std::move(command_queue), std::move(args.thread));
  return node;
}

ConsumerNode::ConsumerNode(std::string_view name, PipelineDirection pipeline_direction,
                           ConsumerStagePtr pipeline_stage, const Format& format,
                           std::shared_ptr<CommandQueue> command_queue,
                           std::shared_ptr<GraphMixThread> mix_thread)
    : Node(name, /*is_meta=*/false, pipeline_stage->reference_clock(), pipeline_direction,
           pipeline_stage, /*parent=*/nullptr),
      format_(format),
      command_queue_(std::move(command_queue)),
      mix_thread_(std::move(mix_thread)),
      consumer_stage_(std::move(pipeline_stage)) {
  set_thread(mix_thread_);
}

void ConsumerNode::Start(ConsumerStage::StartCommand cmd) const { command_queue_->push(cmd); }
void ConsumerNode::Stop(ConsumerStage::StopCommand cmd) const { command_queue_->push(cmd); }

zx::duration ConsumerNode::GetSelfPresentationDelayForSource(const Node* source) const {
  // TODO(fxbug.dev/87651): Implement this.
  return zx::duration(0);
}

void ConsumerNode::DestroySelf() {
  // Deregister from the thread.
  mix_thread_->RemoveConsumer(consumer_stage_);
}

bool ConsumerNode::CanAcceptSourceFormat(const Format& format) const { return format == format_; }
std::optional<size_t> ConsumerNode::MaxSources() const { return 1; }
bool ConsumerNode::AllowsDest() const { return false; }

}  // namespace media_audio
