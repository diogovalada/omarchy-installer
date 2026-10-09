import '@testing-library/jest-dom/vitest';

// jsdom has no layout; Svelte's bind:clientWidth needs the observer to exist.
globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} };
