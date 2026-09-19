import { render, screen } from '@testing-library/svelte';
import { expect, it } from 'vitest';
import BitLockerPreparation from './BitLockerPreparation.svelte';
it('explains late suspension without offering an early encryption action',()=>{
  render(BitLockerPreparation);
  expect(screen.getByRole('complementary')).toHaveTextContent('without decrypting');
  expect(screen.queryByRole('button')).not.toBeInTheDocument();
});
