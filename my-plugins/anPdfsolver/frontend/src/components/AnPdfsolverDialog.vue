<template>
  <section v-if="state.panelOpen" class="anpdfsolver-panel" role="dialog" aria-modal="true" aria-label="PDF analysis">
    <header class="anpdfsolver-header">
      <div>
        <h2>PDF Canvas Agent</h2>
        <p>Session {{ state.analysisSessionId.slice(0, 8) }}</p>
      </div>
      <button type="button" class="ghost" @click="closePanel">Close</button>
    </header>

    <div class="anpdfsolver-grid">
      <aside class="anpdfsolver-card">
        <div class="card-title">
          <h3>Upload</h3>
          <button type="button" @click="pickFiles">Choose PDF</button>
        </div>
        <p class="muted">{{ state.files.length ? `${state.files.length} file(s) selected` : "No PDF selected" }}</p>
        <ul class="file-list">
          <li v-for="job in state.jobs" :key="job.id" :class="{ selected: selectedJob?.id === job.id }">
            <button type="button" @click="selectJob(job.id)">
              <span>{{ job.file.label }}</span>
              <small>{{ job.state }} · {{ job.stage }}</small>
            </button>
          </li>
        </ul>
        <div class="actions">
          <button type="button" :disabled="!queuedCount || state.busy" @click="startBatch(hostContext)">Start batch</button>
          <button type="button" :disabled="!failedCount || state.busy" @click="retryFailed(hostContext)">Retry failed</button>
          <button type="button" class="danger" :disabled="!selectedJob || selectedJob.state !== 'running'" @click="cancelSelected">Cancel</button>
        </div>
      </aside>

      <main class="anpdfsolver-card">
        <div class="card-title">
          <h3>Batch Analysis</h3>
          <span class="status">{{ queuedCount }} queued · {{ runningCount }} running · {{ completedCount }} review · {{ failedCount }} failed</span>
        </div>
        <div v-if="selectedJob" class="job-status">
          <div>
            <strong>{{ selectedJob.file.label }}</strong>
            <p class="muted">{{ selectedJob.stage }}</p>
          </div>
          <progress :value="selectedJob.progress" max="100" />
        </div>

        <section class="stream">
          <h3>Response API SSE</h3>
          <div class="stream-box">
            <p v-if="!selectedJob?.frames.length" class="muted">No model frames yet.</p>
            <article v-for="frame in selectedJob?.frames || []" :key="String(frame.seq || JSON.stringify(frame).slice(0, 32))">
              <strong>{{ frame.type || "frame" }}</strong>
              <code>{{ frame }}</code>
            </article>
          </div>
        </section>

        <section v-if="selectedJob?.error || state.lastError" class="error-box">
          <h3>Error</h3>
          <p>{{ selectedJob?.error || state.lastError }}</p>
        </section>
      </main>

      <aside class="anpdfsolver-card review-card">
        <div class="card-title">
          <h3>Canonical Review</h3>
          <span class="status">{{ reviewItems.length }} item(s)</span>
        </div>
        <p v-if="!selectedJob?.canonical" class="muted">Rust has not returned a canonical proposal for the selected job.</p>
        <template v-else>
          <p class="muted">Digest {{ selectedJob.digest }}</p>
          <div class="review-list">
            <article v-for="item in reviewItems" :key="item.id">
              <strong>{{ item.title }}</strong>
              <p>{{ item.summary }}</p>
            </article>
          </div>
          <div class="actions">
            <button type="button" :disabled="!selectedJob.digest || selectedJob.state !== 'review'" @click="acceptReview(hostContext)">Accept</button>
            <button type="button" class="danger" :disabled="selectedJob.state !== 'review'" @click="rejectReview(hostContext)">Reject</button>
          </div>
        </template>
      </aside>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { HostContext } from "../context";
import { useAnalysisStore } from "../store";

const { hostContext } = defineProps<{ hostContext: HostContext }>();

const {
  state,
  selectedJob,
  queuedCount,
  runningCount,
  completedCount,
  failedCount,
  closePanel,
  pickFiles,
  startBatch,
  retryFailed,
  cancelSelected,
  selectJob,
  acceptReview,
  rejectReview
} = useAnalysisStore();

const reviewItems = computed(() => {
  const canonical = selectedJob.value?.canonical;
  if (!canonical) return [];
  if (canonical.items?.length) return canonical.items;
  return (canonical.operations || []).map((operation, index) => ({
    id: `operation-${index + 1}`,
    title: String(operation.op || "operation"),
    summary: JSON.stringify(operation).slice(0, 240),
    operation
  }));
});
</script>
