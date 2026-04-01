<!-- web/src/lib/components/ConfirmDialog.svelte -->
<script lang="ts">
  export let open = false;
  export let title = 'Are you sure?';
  export let message = '';
  export let confirmLabel = 'Confirm';
  export let onConfirm: () => void;
  export let onCancel: () => void = () => { open = false; };
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="overlay"
    role="presentation"
    on:click={onCancel}
    on:keydown={(e) => e.key === 'Escape' && onCancel()}
  >
    <div class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-dialog-title"
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <h3 id="confirm-dialog-title">{title}</h3>
      {#if message}<p>{message}</p>{/if}
      <div class="actions">
        <button on:click={onCancel}>Cancel</button>
        <button class="danger" on:click={onConfirm}>{confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.6);
    display: flex; align-items: center; justify-content: center;
    z-index: 100;
  }
  .dialog {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 24px;
    min-width: 300px;
    max-width: 480px;
  }
  h3 { margin: 0 0 12px; font-size: 16px; }
  p { color: var(--color-text-muted); font-size: 13px; margin: 0 0 20px; }
  .actions { display: flex; gap: 8px; justify-content: flex-end; }
</style>
