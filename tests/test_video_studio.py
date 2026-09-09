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

    def test_concepts_cannot_be_approved_until_script_is_written(self):
        t = self.action(self.draft(), 'plan_result', script='', concepts=['细节', '场景', '动态'])
        self.assertEqual(len(t['concepts']), 3)
        with self.assertRaises(ValueError):
            self.action(t, 'approve')
        brief = dict(t['brief'], selectedConcept='场景')
        t = self.action(t, 'save', brief=brief, script='')
        self.assertEqual(t['concepts'], [])
        self.assertEqual(t['brief']['selectedConcept'], '场景')
        t = self.action(t, 'plan_result', script='0–10秒：场景展示')
        self.assertEqual(t['concepts'], [])
        self.assertTrue(self.action(t, 'approve')['approved'])

    def test_no_implicit_route(self):
        self.brief['route'] = ''
        t = self.action(self.draft(), 'plan_result', script='script')
        with self.assertRaisesRegex(ValueError, '选择生成路线'):
            self.action(t, 'approve')

    def test_proxy_uses_catalog_reference_without_blocking(self):
        t = self.approved()
        studio.handle('config', {'name': 'grok', 'base_url': 'https://proxy.example', 'api_key': 'test'})
        with patch.object(studio, 'client', side_effect=AssertionError('quote must stay local')):
            t = self.action(t, 'quote')
        self.assertEqual(t['quote']['estimated_cost']['720p'], '1.40')
        self.assertEqual(t['quote']['pricingStatus'], 'reference')
        self.assertEqual(t['quote']['base_url'], 'https://proxy.example')

    def test_unknown_price_can_submit_after_normal_generate_click(self):
        t = self.approved()
        studio.handle('config', {'name': 'grok', 'base_url': 'https://proxy.example', 'api_key': 'test', 'model': 'custom-video'})
        t = self.action(t, 'quote')
        self.assertEqual(t['quote']['pricingStatus'], 'unknown')
        self.assertNotIn('estimated_cost', t['quote'])
        fake = Mock()
        fake.create_video.return_value = 'remote-custom'
        with patch.object(studio, 'client', return_value=fake):
            t = self.action(t, 'submit')
        self.assertEqual(t['status'], 'running')
        fake.create_video.assert_called_once()

    def test_host_model_catalog_is_the_price_source(self):
        t = self.approved()
        catalog = {'grok-imagine-video-1.5': {'unit': 'second', 'currency': 'USD', 'output': {'720p': 0.123}}}
        with patch.dict('os.environ', {'DSIVIO_MEDIA_PRICING': json.dumps(catalog)}):
            t = self.action(t, 'quote')
        self.assertEqual(t['quote']['estimated_cost']['720p'], '1.23')

    def test_actual_http_rejection_and_compatible_job_receipt(self):
        from http.server import BaseHTTPRequestHandler, HTTPServer
        from threading import Thread
        response = {'status': 401, 'body': {'error': {'message': 'invalid credentials'}}}
        request_headers = []
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args): pass
            def do_POST(self):
                request_headers.append(dict(self.headers))
                self.rfile.read(int(self.headers.get('Content-Length', 0)))
                self.send_response(response['status']); self.send_header('Content-Type', 'application/json'); self.end_headers()
                self.wfile.write(json.dumps(response['body']).encode())
        server = HTTPServer(('127.0.0.1', 0), Handler)
        thread = Thread(target=server.serve_forever, daemon=True); thread.start()
        try:
            for code in (400, 401, 402, 403, 404, 422, 429, 500):
                with self.subTest(code=code):
                    response['status'] = code
                    t = self.approved()
                    studio.handle('config', {'name': 'grok', 'base_url': f'http://127.0.0.1:{server.server_port}', 'api_key': 'test'})
                    t = self.action(t, 'quote')
                    t = self.action(t, 'submit', confirmSpend=True)
                    self.assertEqual(t['submission']['httpStatus'], code)
                    self.assertEqual(t['status'], 'uncertain' if code == 500 else 'approved')
                    self.assertEqual(t['submission']['retryable'], code != 500)
                    if code != 500: self.assertNotIn('remote', t)
            response.update(status=401)
            t = self.approved()
            studio.handle('config', {'name': 'grok', 'base_url': f'http://127.0.0.1:{server.server_port}', 'api_key': 'test'})
            t = self.action(t, 'quote'); t = self.action(t, 'submit', confirmSpend=True)
            response.update(status=200, body={'data': {'id': 'compatible-job-123'}})
            t = self.action(t, 'submit', confirmSpend=True)
            self.assertEqual(t['status'], 'running')
            self.assertEqual(t['remote']['id'], 'compatible-job-123')
            self.assertNotIn('error', t)
            self.assertNotIn('submission', t)
            self.assertTrue(request_headers)
            self.assertTrue(all(h.get('User-Agent') == 'dsvideo-plugin/0.1' for h in request_headers))
        finally:
            server.shutdown(); server.server_close(); thread.join()

    def test_poll_authenticates_provider_download_but_not_external_media(self):
        for url, expected_key in [('/v1/videos/job/content', 'test-secret'),
                                  ('https://cdn.example/video.mp4', None),
                                  ('//cdn.example/video.mp4', None)]:
            with self.subTest(url=url):
                t = self.approved()
                with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='job'):
                    t = self.action(t, 'submit', confirmSpend=True)
                result = {'status': 'done', 'video': {'duration': 10, 'url': url}}
                with patch.object(studio.grok.GrokVideoClient, 'get_video', return_value=result), patch.object(studio.grok, 'download_video') as download:
                    t = self.action(t, 'poll')
                self.assertEqual(t['status'], 'succeeded')
                self.assertEqual(download.call_args.kwargs['api_key'], expected_key)

    def test_missing_receipt_stays_uncertain_without_automatic_retry(self):
        t = self.approved()
        with patch.object(studio.grok.GrokVideoClient, '_request', return_value={'ok': True}) as request:
            t = self.action(t, 'submit', confirmSpend=True)
        self.assertEqual(t['status'], 'uncertain')
        self.assertFalse(t['submission']['retryable'])
        request.assert_called_once()

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

    def test_submit_needs_no_extra_confirmation(self):
        t = self.approved()
        with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='remote-new') as network:
            self.assertEqual(self.action(t, 'submit')['status'], 'running')
            network.assert_called_once()

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

    def test_changed_provider_does_not_require_another_quote(self):
        t = self.approved()
        studio.handle('config', {'name': 'grok', 'base_url': 'https://api.x.ai', 'api_key': 'other-secret'})
        with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='remote-new'):
            self.assertEqual(self.action(t, 'submit')['status'], 'running')

    def test_expired_quote_does_not_block_submission(self):
        t = self.approved()
        t['quote']['at'] = 0
        studio.persist(t)
        with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='remote-new'):
            self.assertEqual(self.action(t, 'submit')['status'], 'running')

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

    def test_grok_seven_references_are_all_sent(self):
        b = {**self.brief, 'images': [f'https://example.test/{i}.png' for i in range(7)], 'inputMode': 'reference', 'voiceIds': ['eve', 'leo']}
        payload = studio.request({'brief': b, 'prompt': 'confirmed script'})
        self.assertEqual(len(payload['reference_images']), 7)
        self.assertNotIn('image', payload)
        self.assertEqual(payload['reference_audios'], [{'voice_id': 'eve'}, {'voice_id': 'leo'}])
        self.assertEqual(studio.grok.cost_quote(duration=10, image_count=7)['estimated_cost']['720p'], '1.47')
        for changes in ({'resolution': '1080p'}, {'images': b['images'] + ['https://example.test/8.png']}, {'inputMode': 'image'}):
            with self.assertRaises(ValueError):
                studio.request({'brief': {**b, **changes}, 'prompt': 'confirmed'})

    def test_grok_single_frame_and_silent_requests(self):
        payload = studio.request({'brief': {**self.brief, 'images': ['https://example.test/1.png'], 'inputMode': 'image', 'resolution': '1080p', 'speechMode': 'silent'}, 'prompt': 'confirmed'})
        self.assertIn('image', payload)
        self.assertFalse(payload['generate_audio'])

    def test_minimax_frame_roles_and_reference_media(self):
        b = {**self.brief, 'route': 'minimax', 'resolution': '768P', 'inputMode': 'frames',
             'images': ['https://example.test/front.png', 'https://example.test/end.png'],
             'firstFrame': 'https://example.test/front.png', 'lastFrame': 'https://example.test/end.png'}
        payload = studio.request({'brief': b, 'prompt': 'confirmed'})
        self.assertEqual([v.get('role') for v in payload['content'][1:]], ['first_frame', 'last_frame'])
        with self.assertRaises(ValueError):
            studio.request({'brief': {**b, 'referenceVideos': ['https://example.test/ref.mp4']}, 'prompt': 'confirmed'})
        b = {**self.brief, 'route': 'minimax', 'resolution': '768P', 'inputMode': 'reference', 'ratio': 'adaptive',
             'referenceVideos': ['https://example.test/ref.mp4'], 'referenceAudios': ['https://example.test/ref.wav']}
        payload = studio.request({'brief': b, 'prompt': 'confirmed'})
        self.assertEqual([v.get('role') for v in payload['content'][1:]], ['reference_video', 'reference_audio'])

    def test_completed_task_can_be_revised_and_overwritten(self):
        t = self.approved()
        with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='job-1'):
            t = self.action(t, 'submit')
        result = {'status': 'done', 'video': {'duration': 10, 'url': '/v1/videos/job/content'}}
        with patch.object(studio.grok.GrokVideoClient, 'get_video', return_value=result), patch.object(studio.grok, 'download_video'):
            t = self.action(t, 'poll')
        self.assertEqual(t['status'], 'succeeded')
        task_id = t['id']
        old_output = t['output']
        t = self.action(t, 'plan_result', script='书包完整入画')
        self.assertEqual(t['id'], task_id)
        self.assertEqual(t['status'], 'draft')
        self.assertEqual(t.get('output'), old_output)
        self.assertNotIn('remote', t)
        t = self.action(t, 'approve')
        t = self.action(t, 'prompt_result', prompt='bag fills the frame')
        t = self.action(t, 'quote')
        with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='job-2') as create:
            t = self.action(t, 'submit')
        self.assertEqual(t['remote']['id'], 'job-2')
        create.assert_called_once()
        with patch.object(studio.grok.GrokVideoClient, 'get_video', return_value=result), patch.object(studio.grok, 'download_video'):
            t = self.action(t, 'poll')
        self.assertEqual(t['status'], 'succeeded')
        self.assertEqual(t['output'], old_output)

    def test_in_flight_task_still_cannot_be_rewritten(self):
        t = self.approved()
        with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='job-1'):
            t = self.action(t, 'submit')
        with self.assertRaisesRegex(ValueError, '尚未结束'):
            self.action(t, 'plan_result', script='改掉进行中的任务')

    def test_route_specific_ratios(self):
        for route, ratios in [('grok', studio.grok.RATIOS), ('minimax', studio.mini.RATIOS)]:
            for ratio in ratios:
                studio.validate({'brief': {**self.brief, 'route': route, 'ratio': ratio, 'resolution': '720p' if route == 'grok' else '768P'}})
        with self.assertRaises(ValueError):
            studio.validate({'brief': {**self.brief, 'ratio': '21:9'}})


if __name__ == '__main__':
    unittest.main()
