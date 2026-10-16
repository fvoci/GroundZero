<script lang="ts">
  import HealthStatus from '../lib/ui/HealthStatus.svelte';
  import { fetchHealth } from '../lib/api/client';
</script>

<main>
  <h1>fvoci</h1>
  <HealthStatus load={fetchHealth} />
</main>
