<script lang="ts">
  // 로그인 전 화면. 로그인은 서버의 /auth/login이 OIDC 제공자로 보낸다 (D-45).
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  interface Props {
    oidcEnabled: boolean;
    returnTo: string;
  }

  const { oidcEnabled, returnTo }: Props = $props();
</script>

<main class="mx-auto flex min-h-svh max-w-md flex-col justify-center gap-6 px-4">
  <h1 class="text-2xl font-semibold">fvoci</h1>
  <p>{m.app_login_intro()}</p>
  {#if oidcEnabled}
    <Button href={`/auth/login?return_to=${encodeURIComponent(returnTo)}`} size="lg"
      >{m.app_login()}</Button
    >
  {:else}
    <p role="alert" class="rounded border border-destructive/40 bg-destructive/10 px-3 py-2">
      {m.app_login_unconfigured()}
    </p>
  {/if}
</main>
