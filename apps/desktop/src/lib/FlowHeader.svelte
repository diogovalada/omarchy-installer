<script lang="ts">
  export let eyebrow: string;
  export let title: string;
  export let description: string;
</script>

<header>
  <p>{eyebrow}</p>
  <h1>{title}</h1>
  <div class="sub">{description}</div>
</header>

<style>
  header { max-width: 650px; }
  p { margin: 0 0 12px; color: var(--accent); font-size: 10px; font-weight: 700; letter-spacing: .12em; text-transform: uppercase; }
  h1 { margin: 0; color: var(--text); font-size: clamp(24px, 3vw, 32px); font-weight: 400; letter-spacing: -.05em; line-height: 1.3; }
  .sub { margin-top: 14px; color: var(--text-muted); font-size: 12px; line-height: 1.85; }
</style>
