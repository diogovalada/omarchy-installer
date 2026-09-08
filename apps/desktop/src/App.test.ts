import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import App from './App.svelte';

describe('preview availability', () => {
  it('shows one download surface and keeps native execution unavailable in the browser', async () => {
    render(App);
    expect(screen.queryByRole('button', { name: 'Try' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Manage' })).not.toBeInTheDocument();
    expect(screen.queryByRole('navigation')).not.toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'Download' })).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Download' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Change' })).toBeDisabled();
    expect(screen.getByText(/downloads require the desktop app/)).toBeInTheDocument();
    expect(screen.queryByText('Verified')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Create bootable USB' })).toBeEnabled();
    expect(screen.queryByText('Demo USB Drive')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Install without USB' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Install without USB' })).toHaveAccessibleDescription('Coming soon');
    await fireEvent.click(screen.getByRole('button', { name: 'Install without USB' }));
    expect(screen.queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Create bootable USB' }));
    expect(screen.getByText('Open the desktop app to inspect disks and continue.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Check disks' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Check with administrator access' })).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.getByRole('button', { name: 'Create bootable USB' })).toHaveFocus();
  });
});
