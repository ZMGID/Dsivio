"""Small boundary between mechanical collectors and the supervising Agent."""
from report_state import write_json


def handoff(folder, identity, reason):
    result = {**identity, 'status': 'needs_agent', 'reason': str(reason),
              'checkpoint': str(folder / 'checkpoint.json'),
              'capture': str(folder / 'capture.json')}
    write_json(folder / 'handoff.json', result)
    return result
