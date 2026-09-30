export const generationTypes = { imageSet: "imageSet", videoSet: "videoSet" };
export const sealGeneratedImageSet = (value) => ({ sealed: "image", ...value });
export const sealGeneratedVideoSet = (value) => ({ sealed: "video", ...value });
