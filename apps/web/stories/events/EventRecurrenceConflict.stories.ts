import type { Meta, StoryObj } from '@storybook/svelte';
import { fn } from 'storybook/test';
import FieldConflict from '$lib/features/common/FieldConflict.svelte';
import { ruleConflictFields, type RuleValue } from '$lib/features/events/conflict';

const rule = { freq: 'weekly' as const, interval: 1, weekdays: [1], until: null, count: null };
const meta = {
  title: 'Events/EventRecurrenceConflict',
  component: FieldConflict<RuleValue>,
  args: {
    fields: ruleConflictFields(),
    base: { rule },
    mine: { rule: { ...rule, weekdays: [1, 5] } },
    server: { rule: { ...rule, weekdays: [1, 3], until: '2026-12-31' } },
    choices: {},
    onsave: fn(),
    onedit: fn(),
    oncancel: fn(),
  },
} satisfies Meta<typeof FieldConflict<RuleValue>>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const Dark375Font200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375', isRotated: false } },
};
