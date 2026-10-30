/** A song's picture, or a generated one (initials on a gradient) until or unless there is one. */
export function coverElement(title: string, artist: string, url: Promise<string | null> | string | null, className = ""): HTMLElement {
  const box = document.createElement('div');
  box.className = `cover ${className} placeholder`;
  let hash = 0;
  for (const c of title + artist) hash = (hash * 31 + c.charCodeAt(0)) | 0;
  const hue = Math.abs(hash) % 360;
  box.style.background = `linear-gradient(135deg, hsl(${hue} 70% 46%), hsl(${(hue + 50) % 360} 70% 22%))`;
  box.textContent = initials(title);
  void Promise.resolve(url).then((src) => {
    if (!src) return;
    const img = document.createElement('img');
    img.alt = '';
    img.src = src;
    img.onload = () => {
      box.classList.remove('placeholder');
      box.style.background = '';
      box.replaceChildren(img);
    };
  });
  return box;
}

export function initials(title: string): string {
  const words = title.split(/[\s\-–—_()[\]]+/).filter((w) => /\p{L}|\d/u.test(w));
  return (words.slice(0, 2).map((w) => w[0]).join('') || '♪').toUpperCase();
}
