import { describe, expect, it } from 'vitest'
import {
  applyVideoStudioDrop,
  looksLikeAudioPath,
  looksLikeVideoPath,
  videoDropZoneFromElement,
} from './videoDrop'

describe('video studio drop paths', () => {
  it('recognizes common video files and rejects folders', () => {
    expect(looksLikeVideoPath('C:\\clips\\demo.MP4')).toBe(true)
    expect(looksLikeVideoPath('/Users/me/ref.webm')).toBe(true)
    expect(looksLikeVideoPath('C:\\clips\\folder')).toBe(false)
    expect(looksLikeVideoPath('C:\\goods\\front.png')).toBe(false)
    expect(looksLikeAudioPath('C:\\voice\\line.MP3')).toBe(true)
    expect(looksLikeAudioPath('C:\\voice\\line.wav')).toBe(true)
    expect(looksLikeAudioPath('C:\\clips\\demo.mp4')).toBe(false)
  })

  it('appends unique images in creation and takes the first video in analysis', () => {
    expect(
      applyVideoStudioDrop(
        'creation',
        { images: ['C:\\a.png'], source: '' },
        ['C:\\a.png', 'C:\\b.jpg', 'C:\\skip.mp4'],
      ),
    ).toEqual({ images: ['C:\\a.png', 'C:\\b.jpg'] })
    expect(
      applyVideoStudioDrop(
        'analysis',
        { images: [], source: '' },
        ['C:\\note.txt', 'C:\\first.mov', 'C:\\second.mp4'],
      ),
    ).toEqual({ source: 'C:\\first.mov' })
  })

  it('rejects the wrong media for the current page', () => {
    expect(
      applyVideoStudioDrop('creation', { images: [], source: '' }, ['C:\\clip.mp4']),
    ).toEqual({ error: '请拖入商品图片（PNG / JPG / WebP）' })
    expect(
      applyVideoStudioDrop('analysis', { images: [], source: '' }, ['C:\\front.png']),
    ).toEqual({ error: '请拖入视频文件（MP4 / MOV / WebM）' })
    expect(
      applyVideoStudioDrop('templates', { images: [], source: '' }, ['C:\\front.png']),
    ).toEqual({ error: '当前页不能导入素材' })
  })
})


it('imports both reference video and product images for remake without losing existing images', () => {
  expect(applyVideoStudioDrop('remake', { images: ['/old.png'], source: '' }, ['/reference.mp4', '/new.jpg'])).toEqual({ source: '/reference.mp4', images: ['/old.png', '/new.jpg'] })
})

it('routes remake drops to the hovered zone', () => {
  expect(
    applyVideoStudioDrop(
      'remake',
      { images: ['/old.png'], source: '/old.mp4' },
      ['/new.jpg', '/skip.mp4'],
      'images',
    ),
  ).toEqual({ images: ['/old.png', '/new.jpg'] })
  expect(
    applyVideoStudioDrop(
      'remake',
      { images: ['/old.png'], source: '/old.mp4' },
      ['/new.jpg', '/next.mov'],
      'source',
    ),
  ).toEqual({ source: '/next.mov' })
  expect(
    applyVideoStudioDrop('remake', { images: [], source: '' }, ['/clip.mp4'], 'images'),
  ).toEqual({ error: '请拖入商品图片（PNG / JPG / WebP）' })
})

it('appends reference videos and audio onto the hovered minimax slots', () => {
  expect(
    applyVideoStudioDrop(
      'creation',
      { images: [], source: '', referenceVideos: ['/a.mp4'] },
      ['/b.mov', '/skip.png'],
      'referenceVideos',
    ),
  ).toEqual({ referenceVideos: ['/a.mp4', '/b.mov'] })
  expect(
    applyVideoStudioDrop(
      'creation',
      { images: [], source: '', referenceAudios: ['/a.mp3'] },
      ['/b.wav'],
      'referenceAudios',
    ),
  ).toEqual({ referenceAudios: ['/a.mp3', '/b.wav'] })
  expect(
    applyVideoStudioDrop(
      'creation',
      { images: [], source: '', referenceVideos: ['/1.mp4', '/2.mp4', '/3.mp4'] },
      ['/4.mp4'],
      'referenceVideos',
    ),
  ).toEqual({ error: '参考视频最多 3 段' })
})

it('hits the video drop zone from a nested child', () => {
  const child = {
    closest: (selector: string) => (selector.includes('images') ? {} : null),
  } as unknown as Element
  expect(videoDropZoneFromElement(child)).toBe('images')
  expect(videoDropZoneFromElement({ closest: () => null } as unknown as Element)).toBeNull()
})
