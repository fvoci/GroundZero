<script lang="ts">
  // 로그인 전 화면. 로그인은 서버의 /auth/login이 OIDC 제공자로 보낸다 (D-45).
  import { Button } from '$lib/ui/button';

  interface Props {
    oidcEnabled: boolean;
    returnTo: string;
  }

  const { oidcEnabled, returnTo }: Props = $props();
</script>

<main class="mx-auto flex min-h-svh max-w-md flex-col justify-center gap-6 px-4">
  <h1 class="text-2xl font-semibold">fvoci</h1>
  <p>작업·지식·학습을 한곳에서 관리합니다. 계속하려면 로그인하세요.</p>
  {#if oidcEnabled}
    <Button href={`/auth/login?return_to=${encodeURIComponent(returnTo)}`} size="lg">로그인</Button>
  {:else}
    <p role="alert" class="rounded border border-destructive/40 bg-destructive/10 px-3 py-2">
      로그인 제공자가 설정되지 않았습니다. 운영자는 FVOCI_OIDC_ISSUER 등을 설정해야 합니다.
    </p>
  {/if}
</main>
