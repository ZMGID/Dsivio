import { describe, expect, it } from 'vitest'
import { applyVideoStudioDrop, looksLikeVideoPath } from './videoDrop'

describe('video studio drop paths', () => {
  it('recognizes common video files and rejects folders', () => {
    expect(looksLikeVideoPath('C:\\clips\\demo.MP4')).toBe(true)
    expect(looksLikeVideoPath('/Users/me/ref.webm')).toBe(true)
    expect(looksLikeVideoPath('C:\\clips\\folder')).toBe(false)
    expect(looksLikeVideoPath('C:\\goods\\front.png')).toBe(false)
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
