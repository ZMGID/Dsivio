"""Chat-authored results round-trip through the same worker CLI as the desktop."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / 'src-tauri/resources/plugins/dsvideo-plugin/scripts/studio.py'


class ChatResultActionsTests(unittest.TestCase):
    def test_confirmed_script_can_resume_through_prompt_and_quote_without_resubmission(self):
        with tempfile.TemporaryDirectory(prefix='video chat ') as directory:
            root = Path(directory)
            env = dict(os.environ, DSVIDEO_STUDIO_ROOT=str(root / 'workspace'),
                       DSVIDEO_CONFIG_PATH=str(root / 'providers.json'),
                       DSIVIO_MEDIA_PRICING='{}')

            def call(action, data):
                result = subprocess.run(
                    [sys.executable, '-s', '-B', '-X', 'utf8', str(SCRIPT), action],
                    input=json.dumps(data), text=True, capture_output=True,
                    env=env, cwd=root, timeout=20,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                return json.loads(result.stdout)

            def update(task, action, **values):
                return call(action, dict(id=task['id'], revision=task['revision'], **values))

            # Local quoting requires a configured provider; this fixture never submits.
            call('config', {'name': 'grok', 'base_url': 'https://api.x.ai',
                            'api_key': 'test-only-not-a-real-key'})
            task = call('create', {'brief': dict(
                name='logo animation', mode='creation', request='animate logo', images=[],
                duration=5, ratio='16:9', route='grok', resolution='720p',
                language='zh-CN', source='',
            )})
            task_id = task['id']
            task = update(task, 'plan_result', script='0–5秒：标志从静止开始旋转。')
            task = update(task, 'approve')
            # This is the exact persisted state where the reported chat got stuck.
            task = call('get', {'id': task_id})
            self.assertTrue(task['approved'])
            self.assertEqual(task['prompt'], '')
            task = update(task, 'prompt_result', prompt='One continuous five-second shot of the logo rotating.')
            task = update(task, 'quote')
            resumed = call('get', {'id': task_id})
            self.assertEqual(resumed, task)
            self.assertEqual(resumed['id'], task_id)
            self.assertEqual(resumed['status'], 'approved')
            self.assertTrue(resumed['prompt'])
            self.assertIsNotNone(resumed['quote'])
            self.assertNotIn('remote', resumed)
            self.assertEqual(len(list((root / 'workspace/tasks').glob('*.json'))), 1)

            # Analysis results also pass through the public result-saving path.
            evidence = {'warnings': ['audio unavailable']}
            task = update(task, 'analysis_result', script='画面：标志旋转。音频缺失。', analysis=evidence)
            self.assertEqual(task['analysis'], evidence)
            self.assertFalse(task['approved'])
            self.assertEqual(task['prompt'], '')
            self.assertIsNone(task['quote'])


if __name__ == '__main__':
    unittest.main()
