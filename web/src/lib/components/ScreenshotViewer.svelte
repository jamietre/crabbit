<!-- web/src/lib/components/ScreenshotViewer.svelte -->
<script lang="ts">
  export let base64: string;
  export let filename: string = 'screenshot';
</script>

<div class="viewer">
  <div class="label">{filename}</div>
  <img
    src="data:image/png;base64,{base64}"
    alt={filename}
    loading="lazy"
  />
</div>

<style>
  .viewer {
    border: 1px solid var(--color-border);
    border-radius: 8px;
    overflow: hidden;
  }
  .label {
    background: var(--color-surface);
    padding: 4px 10px;
    font-size: 11px;
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border);
  }
  img {
    display: block;
    width: 100%;
    max-height: 600px;
    object-fit: contain;
    background: #000;
  }
</style>
