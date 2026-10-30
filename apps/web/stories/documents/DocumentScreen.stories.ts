import { activityApi } from '$lib/features/activity/api';
import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, mocked, spyOn, waitFor } from 'storybook/test';
import { getSchema } from '@tiptap/core';
import { prosemirrorJSONToYXmlFragment } from '@tiptap/y-tiptap';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';
import { api } from '$lib/api/client';
import type { DocNode } from '$lib/api/client';
import { schemaExtensions } from '$lib/features/documents/editor-schema';
import * as live from '$lib/features/documents/live';
import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { ACCESS_DOCUMENT, ACCESS_FILE } from '../fixtures/access';
import { THREAD } from '../fixtures/comments';
import { setUpScreen } from '../fixtures/screens';

const meta = {
  title: 'Documents/DocumentScreen',
  component: DocumentScreen,
  args: { documentId: ACCESS_DOCUMENT.id, workspaces: [] },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => {
    const cleanup = setUpScreen('/');
    spyOn(api, 'document').mockResolvedValue(ACCESS_DOCUMENT);
    spyOn(api, 'documentFiles').mockResolvedValue([ACCESS_FILE]);
    spyOn(api, 'documentBacklinks').mockResolvedValue([]);
    spyOn(activityApi, 'comments').mockResolvedValue(THREAD);
    return cleanup;
  },
} satisfies Meta<typeof DocumentScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
const mathBody: DocNode = {
  type: 'doc',
  content: [
    { type: 'paragraph', content: [{ type: 'inlineMath', attrs: { latex: 'E = mc^2' } }] },
    { type: 'blockMath', attrs: { latex: '\\sum_{i=1}^n i' } },
  ],
};
export const Owner: Story = {
  beforeEach: () => {
    spyOn(api, 'document').mockResolvedValue({ ...ACCESS_DOCUMENT, body: mathBody });
    // The product editor reads its body from Yjs. Seed synthetic content without
    // connecting to a server; collaboration/authentication remain browser regressions.
    const opened = mocked(live.openLive).mockImplementation((_id, user) => {
      const document = new Y.Doc();
      prosemirrorJSONToYXmlFragment(
        getSchema(schemaExtensions()),
        mathBody,
        document.getXmlFragment('default'),
      );
      return {
        document,
        provider: new WebsocketProvider('ws://localhost', 'story', document, {
          connect: false,
          disableBc: true,
        }),
        user,
        persisted: { on: () => undefined, off: () => undefined },
      };
    });
    return () => {
      opened.mockReset();
    };
  },
  play: async ({ canvasElement }) => {
    await waitFor(
      async () => {
        await expect(canvasElement.querySelectorAll('.tiptap .katex')).toHaveLength(2);
      },
      { timeout: 10_000 },
    );
  },
};
export const NarrowDarkLargeText: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
