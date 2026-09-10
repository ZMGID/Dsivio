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

    def test_user_prompt_is_preserved_through_approval_and_request_without_conversion(self):
        original = '  固定机位，保持商品外观。\nSay exactly: "Hello".  '
        for route, resolution in [('grok', '720p'), ('minimax', '768P')]:
            with self.subTest(route=route):
                task = studio.handle('create', {'brief': {**self.brief, 'route': route, 'resolution': resolution, 'request': original}})
                task = self.action(task, 'save', brief=task['brief'], script=original)
                task = self.action(task, 'approve')
                if route != 'grok':
                    task = self.action(task, 'prompt_result', prompt=original)
                payload = studio.request(task)
                submitted = payload['prompt'] if route == 'grok' else payload['content'][0]['text']
                self.assertEqual(submitted, original)
                self.assertEqual(task['script'], original)

    def test_preflight_blocks_missing_generation_options_before_planning(self):
        for field, value in [('route', ''), ('resolution', ''), ('duration', 2.5)]:
            with self.subTest(field=field):
                t = studio.handle('create', {'brief': {**self.brief, field: value}})
                with self.assertRaises(ValueError):
                    self.action(t, 'preflight', operation='plan')
                self.assertEqual(studio.read(studio.task_path(t['id']))['revision'], t['revision'])

    def test_analysis_preflight_requires_source_but_not_generation_settings(self):
        t = studio.handle('create', {'brief': {**self.brief, 'mode': 'analysis', 'route': '', 'resolution': '', 'source': ''}})
        with self.assertRaisesRegex(ValueError, '参考视频'):
            self.action(t, 'preflight', operation='analyze')
        t['brief']['source'] = '/reference.mp4'
        studio.persist(t)
        self.assertEqual(self.action(t, 'preflight', operation='analyze')['revision'], t['revision'])

    def test_preflight_blocks_missing_credentials_and_unapproved_conversion(self):
        t = self.draft()
        with self.assertRaisesRegex(ValueError, 'API Key'):
            self.action(t, 'preflight', operation='plan')
        studio.handle('config', {'name': 'grok', 'base_url': 'https://api.x.ai', 'api_key': 'test'})
        self.assertEqual(self.action(t, 'preflight', operation='plan')['revision'], t['revision'])
        with self.assertRaisesRegex(ValueError, '确认当前剧本'):
            self.action(t, 'preflight', operation='prepare')

    def approved(self):
        t = self.action(self.draft(), 'plan_result', script='0–10 seconds: product on table')
        t = self.action(t, 'approve')
        t = self.action(t, 'prompt_result', prompt='Product on table for ten seconds.')
        studio.handle('config', {'name': 'grok', 'base_url': 'https://api.x.ai', 'api_key': 'test-secret'})
        return self.action(t, 'quote')

    @unittest.skipIf(sys.platform == 'win32', 'POSIX flock regression')
    def test_task_reads_do_not_wait_for_the_workspace_lock(self):
        import fcntl
        import os
        import subprocess
        t = self.draft()
        with (self.root / '.lock').open('a+b') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            result = subprocess.run([sys.executable, '-B', str(SCRIPTS / 'studio.py'), 'get'],
                input=json.dumps({'id': t['id']}), capture_output=True, text=True, timeout=5,
                env={**os.environ, 'DSVIDEO_STUDIO_ROOT': str(self.root)})
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual(json.loads(result.stdout)['id'], t['id'])

    def test_comfy_preflight_retains_the_actionable_error(self):
        self.brief.update(route='comfy', resolution='0.5')
        t = self.draft()
        t = self.action(t, 'plan_result', script='product shot')
        t = self.action(t, 'approve')
        t = self.action(t, 'prompt_result', prompt='product shot')
        t = self.action(t, 'submit')
        t = self.action(t, 'preflight_failed', detail='upload_file: connection refused')
        self.assertEqual(t['status'], 'approved')
        self.assertIn('upload_file: connection refused', t['error'])

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

    def test_grok_approval_uses_script_without_conversion(self):
        script = '0–10秒：展示商品。对白：Olá!'
        t = self.action(self.draft(), 'plan_result', script=script)
        t = self.action(t, 'approve')
        self.assertEqual(t['prompt'], script)

    def test_h3_approval_still_requires_conversion(self):
        self.brief.update(route='minimax', resolution='768P')
        t = self.action(self.draft(), 'plan_result', script='商品展示')
        t = self.action(t, 'approve')
        self.assertEqual(t['prompt'], '')

    def test_grok_does_not_impose_an_unverified_byte_limit(self):
        script = '灯光展示' * 500
        t = self.action(self.draft(), 'plan_result', script=script)
        t = self.action(t, 'approve')
        self.assertGreater(len(script.encode('utf-8')), 4096)
        self.assertEqual(t['prompt'], script)
        self.assertEqual(studio.request(t)['prompt'], script)

    def test_grok_retry_replaces_legacy_converted_prompt(self):
        t = self.approved()
        t = self.action(t, 'prompt_result', prompt='x' * 6137)
        with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='job') as create:
            t = self.action(t, 'submit')
        self.assertEqual(create.call_args.args[0]['prompt'], t['script'])
        self.assertEqual(t['prompt'], t['script'])

    def test_deleted_task_reports_missing_record_not_network_failure(self):
        t = self.draft()
        studio.task_path(t['id']).unlink()
        for action in ('get', 'save', 'submit'):
            with self.subTest(action=action), self.assertRaisesRegex(ValueError, 'VIDEO_TASK_NOT_FOUND'):
                self.action(t, action, brief=self.brief)
        self.assertFalse(studio.task_path(t['id']).exists())

    def test_supplier_prompt_limit_is_not_hidden_by_generic_400(self):
        failure = studio.submission_failure(studio.grok.ApiError(
            'Prompt length exceeds the maximum allowed length of 4096', http_status=400))
        self.assertIn('Prompt length exceeds the maximum allowed length of 4096', failure['reason'])
        self.assertTrue(failure['retryable'])

    def test_length_rejection_preserves_script_and_records_encoding_counts(self):
        t = self.approved()
        script = '灯' * 1827
        t = self.action(t, 'plan_result', script=script)
        t = self.action(t, 'approve')
        error = studio.grok.ApiError('Prompt length exceeds the maximum allowed length of 4096', http_status=400)
        with patch.object(studio.grok.GrokVideoClient, 'create_video', side_effect=error):
            t = self.action(t, 'submit')
        self.assertEqual(t['script'], script)
        self.assertEqual(t['prompt'], script)
        self.assertEqual(t['submission']['promptCharacters'], 1827)
        self.assertEqual(t['submission']['promptUtf8Bytes'], 5481)
        self.assertIn('UTF-8', t['error'])
        self.assertEqual(t['status'], 'approved')

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
                with patch.object(studio.grok.GrokVideoClient, 'get_video', return_value=result), patch.object(studio.grok, 'download_video') as download, patch.object(studio, 'probe_video', return_value={'width': 720, 'height': 1280, 'duration': 10.04, 'hasAudio': True}):
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

    def test_grok_single_product_image_defaults_to_appearance_reference(self):
        b = {**self.brief, 'images': ['https://example.test/product.png'], 'inputMode': 'auto'}
        with patch.object(studio, 'prepare_grok_frame') as frame:
            payload = studio.request({'brief': b, 'prompt': 'product scene'})
        frame.assert_not_called()
        self.assertEqual(payload['reference_images'], [{'url': b['images'][0]}])
        self.assertNotIn('image', payload)
        with self.assertRaisesRegex(ValueError, '720p'):
            studio.request({'brief': {**b, 'resolution': '1080p'}, 'prompt': 'product scene'})

    def test_grok_single_frame_and_silent_requests(self):
        with patch.object(studio, 'prepare_grok_frame', return_value='https://example.test/prepared.png'):
            payload = studio.request({'brief': {**self.brief, 'images': ['https://example.test/1.png'], 'inputMode': 'image', 'resolution': '1080p', 'speechMode': 'silent'}, 'prompt': 'confirmed'})
        self.assertIn('image', payload)
        self.assertFalse(payload['generate_audio'])

    def test_grok_frame_preserves_product_and_requested_canvas(self):
        source = self.root / 'square.png'
        studio.subprocess.run(['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'color=red:s=128x128',
                               '-frames:v', '1', str(source)], check=True)
        task = self.draft()
        task['brief'].update(images=[str(source)], resolution='720p', ratio='9:16')
        frame = studio.prepare_grok_frame(task)
        media = studio.probe_video(frame)
        self.assertEqual((media['width'], media['height']), (720, 1280))
        self.assertEqual(studio.probe_video(source)['width'], 128)
        task['brief'].update(resolution='1080p', ratio='16:9')
        frame = studio.prepare_grok_frame(task)
        media = studio.probe_video(frame)
        self.assertEqual((media['width'], media['height']), (1920, 1080))

    def test_grok_output_contract_uses_actual_media(self):
        requested = dict(aspect_ratio='9:16', resolution='720p', duration=5, generate_audio=True)
        self.assertEqual(studio.output_mismatches(dict(width=720, height=1280, duration=5.04, hasAudio=True), requested), [])
        issues = studio.output_mismatches(dict(width=544, height=544, duration=3, hasAudio=False), requested)
        self.assertEqual(len(issues), 4)
        self.assertIn('画幅不符', issues[0])
        self.assertEqual(studio.output_mismatches(dict(width=720, height=1280, duration=5, hasAudio=False), dict(requested, generate_audio=False)), [])

    def test_grok_wrong_output_is_preserved_but_not_successful(self):
        task = self.approved()
        with patch.object(studio.grok.GrokVideoClient, 'create_video', return_value='job'):
            task = self.action(task, 'submit')
        self.assertEqual(task['requested']['aspect_ratio'], '9:16')
        self.assertTrue(task['requested']['generate_audio'])
        with patch.object(studio.grok.GrokVideoClient, 'get_video', return_value={'status':'done', 'video':{'url':'/video.mp4'}}), \
             patch.object(studio.grok, 'download_video'), \
             patch.object(studio, 'probe_video', return_value=dict(width=960, height=960, duration=10, hasAudio=False)):
            task = self.action(task, 'poll')
        self.assertEqual(task['status'], 'failed')
        self.assertIn('画幅不符', task['error'])
        self.assertIn('没有音轨', task['error'])
        self.assertTrue(task['output'])
        self.assertEqual(task['remote']['id'], 'job')

    def test_reference_template_keeps_measured_ratio_without_invalid_generation_duration(self):
        task = self.draft()
        task = self.action(task, 'save', brief=dict(task['brief'], mode='analysis'))
        metadata = dict(width=576, height=1024, duration=20.04, hasAudio=True)
        task = self.action(task, 'analysis_result', script='portrait reference', analysis={'content':[
            {'type':'text', 'text':json.dumps({'metadata':metadata, 'transcript':[]})}]})
        template = self.action(task, 'template_save', name='reference')
        self.assertEqual(template['spec']['aspect_ratio'], '9:16')
        self.assertTrue(template['spec']['source_has_audio'])
        self.assertEqual(template['spec']['source_duration_seconds'], 20.04)
        self.assertNotIn('duration_seconds', template['spec'])

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
        with patch.object(studio.grok.GrokVideoClient, 'get_video', return_value=result), patch.object(studio.grok, 'download_video'), patch.object(studio, 'probe_video', return_value={'width': 720, 'height': 1280, 'duration': 10.04, 'hasAudio': True}):
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
        with patch.object(studio.grok.GrokVideoClient, 'get_video', return_value=result), patch.object(studio.grok, 'download_video'), patch.object(studio, 'probe_video', return_value={'width': 720, 'height': 1280, 'duration': 10.04, 'hasAudio': True}):
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
