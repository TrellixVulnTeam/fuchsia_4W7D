// Copyright 2022 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#ifndef SRC_MEDIA_AUDIO_SERVICES_MIXER_MIX_TESTING_FAKE_PIPELINE_STAGE_H_
#define SRC_MEDIA_AUDIO_SERVICES_MIXER_MIX_TESTING_FAKE_PIPELINE_STAGE_H_

#include <lib/syslog/cpp/macros.h>

#include <memory>
#include <optional>
#include <string>
#include <unordered_set>

#include "src/media/audio/services/common/logging.h"
#include "src/media/audio/services/mixer/mix/pipeline_stage.h"
#include "src/media/audio/services/mixer/mix/testing/defaults.h"

namespace media_audio {

class FakePipelineStage;
using FakePipelineStagePtr = std::shared_ptr<FakePipelineStage>;

// A very simple no-op implementation of PipelineStage.
class FakePipelineStage : public PipelineStage {
 public:
  struct Args {
    std::string name;
    std::optional<Format> format;                    // if unspecified, use an arbitrary default
    std::optional<UnreadableClock> reference_clock;  // if unspecified, use an arbitrary default
  };
  static FakePipelineStagePtr Create(Args args) {
    if (!args.format) {
      args.format = Format::CreateOrDie({::fuchsia_audio::SampleType::kFloat32, 2, 48000});
    }
    if (!args.reference_clock) {
      args.reference_clock = DefaultClock();
    }
    return FakePipelineStagePtr(
        new FakePipelineStage(args.name, *args.format, std::move(*args.reference_clock)));
  }

  const std::unordered_set<PipelineStagePtr>& sources() const { return sources_; }
  int64_t max_downstream_consumers() const { return max_downstream_consumers_; }

  // Implementation of PipelineStage.
  void AddSource(PipelineStagePtr source, AddSourceOptions options) { sources_.insert(source); }
  void RemoveSource(PipelineStagePtr source) {
    FX_CHECK(sources_.count(source) > 0);
    sources_.erase(source);
  }
  void UpdatePresentationTimeToFracFrame(std::optional<TimelineFunction> f) {
    set_presentation_time_to_frac_frame(f);
    for (auto& s : sources_) {
      s->UpdatePresentationTimeToFracFrame(f);
    }
  }
  void set_max_downstream_consumers(int64_t n) final { max_downstream_consumers_ = n; }

  // Sets the canned packet to return from Read.
  void SetPacketForRead(std::optional<PacketView> packet) { packet_ = packet; }

 private:
  FakePipelineStage(std::string_view name, Format format, UnreadableClock reference_clock)
      : PipelineStage(name, format, std::move(reference_clock)) {}

  // Implementation of PipelineStage.
  void AdvanceSelfImpl(Fixed frame) {}
  void AdvanceSourcesImpl(MixJobContext& ctx, Fixed frame) {}
  std::optional<Packet> ReadImpl(MixJobContext& ctx, Fixed start_frame, int64_t frame_count) {
    if (!packet_) {
      return std::nullopt;
    }
    auto isect = packet_->IntersectionWith(start_frame, frame_count);
    if (!isect) {
      return std::nullopt;
    }
    return MakeUncachedPacket(isect->start(), isect->length(), isect->payload());
  }

  std::unordered_set<PipelineStagePtr> sources_;
  std::optional<PacketView> packet_;
  int64_t max_downstream_consumers_ = 0;
};

}  // namespace media_audio

#endif  // SRC_MEDIA_AUDIO_SERVICES_MIXER_MIX_TESTING_FAKE_PIPELINE_STAGE_H_
