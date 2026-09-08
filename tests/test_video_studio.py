"""Shared workspace integration and paid-call safety, with no network requests."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch, Mock

SCRIPTS = Path(__file__).resolve().parents[1] / 'src-tauri/resources/plugins/dsvideo-plugin/scripts'
sys.path.insert(0, str(SCRIPTS))
import studio


class WorkspaceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.root_patch = patch.object(studio, 'ROOT', self.root)
        self.root_patch.start()
        self.config_patch = patch.dict('os.environ', {'DSVIDEO_CONFIG_PATH': str(self.root / 'providers.json')})
        self.config_patch.start()
        studio.bootstrap()
        self.brief = dict(name='test', mode='creation', request='show product', images=[], duration=10,
                          ratio='9:16', route='grok', resolution='720p', language='pt-BR', source='')

    def tearDown(self):
        self.config_patch.stop()
        self.root_patch.stop()
        self.tmp.cleanup()

    def action(self, t, action, **kw):
        return studio.handle(action, dict(id=t['id'], revision=t['revision'], **kw))

    def draft(self):
        return studio.handle('create', {'brief': self.brief})

    def approved(self):
        t = self.action(self.draft(), 'plan_result', script='0–10 seconds: product on table')
        t = self.action(t, 'approve')
        t = self.action(t, 'prompt_result', prompt='Product on table for ten seconds.')
        studio.handle('config', {'name': 'grok', 'base_url': 'https://api.x.ai', 'api_key': 'test-secret'})
        return self.action(t, 'quote')

    def test_no_implicit_route(self):
        self.brief['route'] = ''
        t = self.action(self.draft(), 'plan_result', script='script')
        with self.assertRaisesRegex(ValueError, '选择生成路线'):
            self.action(t, 'approve')

    def test_edit_invalidates_approval_prompt_and_quote(self):
        t = self.approved()
        t = self.action(t, 'save', brief=self.brief, script='new script')
        self.assertFalse(t['approved'])
        self.assertFalse(t['prompt'])
        self.assertIsNone(t['quote'])
        with self.assertRaises(ValueError):
            self.action(t, 'submit', confirmSpend=True)

    def test_stale_revision_rejected(self):
        t = self.draft()
        self.action(t, 'plan_result', script='first')
        with self.assertRaisesRegex(ValueError, '其他窗口'):
            self.action(t, 'plan_result', script='stale')

    def test_submit_requires_spend_confirmation(self):
        t = self.approved()
        with patch.object(studio.grok.GrokVideoClient, 'create_video') as network:
            with self.assertRaises(ValueError):
                self.action(t, 'submit')
            network.assert_not_called()

    def test_lost_receipt_is_not_resubmitted(self):
        t = self.approved()
        with patch.object(studio.grok.GrokVideoClient, 'create_video', side_effect=TimeoutError) as network:
            t = self.action(t, 'submit', confirmSpend=True)
            self.assertEqual(t['status'], 'uncertain')
            with self.assertRaises(ValueError):
                self.action(t, 'submit', confirmSpend=True)
            self.assertEqual(network.call_count, 1)

    def test_remote_id_durable_before_recovery(self):
        t = self.approved()
        with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='remote-123'):
            t = self.action(t, 'submit', confirmSpend=True)
        reloaded = studio.handle('get', {'id': t['id']})
        self.assertEqual(reloaded['remote']['id'], 'remote-123')
        with patch.object(studio.grok.GrokVideoClient, 'get_video', return_value={'status': 'pending'}) as query:
            self.action(reloaded, 'poll')
            query.assert_called_once_with('remote-123')

    def test_changed_provider_invalidates_quote(self):
        t = self.approved()
        studio.handle('config', {'name': 'grok', 'base_url': 'https://api.x.ai', 'api_key': 'other-secret'})
        with self.assertRaisesRegex(ValueError, '配置已改变'):
            self.action(t, 'submit', confirmSpend=True)

    def test_expired_quote_cannot_submit(self):
        t = self.approved()
        t['quote']['at'] = 0
        studio.persist(t)
        with self.assertRaisesRegex(ValueError, '已过期'):
            self.action(t, 'submit', confirmSpend=True)

    def test_chat_import_visible_and_unverified(self):
        p = self.root / 'chat-template.json'
        p.write_text(json.dumps({'name': 'chat reference', 'script': 'pan shot'}))
        template = studio.handle('template_import', {'path': str(p)})
        self.assertEqual(template['kind'], 'reference')
        self.assertTrue(any(t['id'] == template['id'] for t in studio.bootstrap()['templates']))

    def test_generated_template_requires_completed_output(self):
        with self.assertRaisesRegex(ValueError, '确认成片'):
            self.action(self.draft(), 'template_save', name='premature', approvedOutput=True)

    def test_config_and_tasks_do_not_return_keys(self):
        self.approved()
        self.assertNotIn('test-secret', json.dumps(studio.bootstrap()))
        self.assertNotIn('test-secret', ''.join(p.read_text() for p in (self.root / 'tasks').glob('*.json')))


if __name__ == '__main__':
    unittest.main()
