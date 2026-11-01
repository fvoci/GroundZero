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
function seedBody(body: DocNode): () => void {
  spyOn(api, 'document').mockResolvedValue({ ...ACCESS_DOCUMENT, body });
  // The product editor reads its body from Yjs. Seed synthetic content without
  // connecting to a server; collaboration/authentication remain browser regressions.
  const opened = mocked(live.openLive).mockImplementation((_id, user) => {
    const document = new Y.Doc();
    prosemirrorJSONToYXmlFragment(
      getSchema(schemaExtensions()),
      body,
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
}
export const Owner: Story = {
  beforeEach: () => seedBody(mathBody),
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
  beforeEach: () =>
    seedBody({
      type: 'doc',
      content: [
        {
          type: 'paragraph',
          content: [
            {
              type: 'text',
              text: '한국어 긴 문서의 줄바꿈과 읽기 흐름을 확인합니다. 연구·프로젝트 노트와 숫자 0123456789, 문장 부호와 기호 → ✓를 함께 보여 줍니다. '.repeat(
                3,
              ),
            },
          ],
        },
        {
          type: 'paragraph',
          content: [
            {
              type: 'text',
              marks: [{ type: 'bold' }],
              text: '굵은 글자도 같은 범위를 덮습니다. 옛한글 ᄒᆞᆫᄀᆞᆯ, 漢字 龍을 빠짐없이 읽습니다.',
            },
          ],
        },
        {
          type: 'paragraph',
          content: [{ type: 'text', text: '보통 굵기의 옛한글 ᄀᆞ와 漢字 龍, Latin café — 200%.' }],
        },
        {
          type: 'codeBlock',
          content: [{ type: 'text', text: 'const 범위 = "KS X 1001 한글 2350자";' }],
        },
      ],
    }),
  play: async ({ canvasElement }) => {
    await waitFor(async () => {
      await expect(canvasElement.querySelector('.tiptap')).toHaveTextContent('옛한글');
      await expect(canvasElement.querySelector('.tiptap strong')).toHaveTextContent('漢字 龍');
      await expect(canvasElement.querySelector('.tiptap pre')).toHaveTextContent('KS X 1001');
    });
  },
};
