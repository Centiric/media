# media
Ses işleme

`ffmpeg` ile `.wav` dosyanızı aşağıdaki özelliklere uygun şekilde dönüştürebilirsiniz:

---

### 🎯 **Hedef Format Özellikleri:**

* Codec: PCM **u-law** veya **A-law**
* Örnekleme hızı (Sample Rate): 8000 Hz
* Kanal: Mono
* Bit derinliği: 8-bit

---

### 🛠️ **u-law (G.711 µ-law) için komut:**

```bash
ffmpeg -i audio/orjinal/welcome.wav -ar 8000 -ac 1 -c:a pcm_mulaw audio/processed/mulaw/welcome.wav
```

### 🛠️ **A-law için komut:**

```bash
ffmpeg -i audio/orjinal/welcome.wav -ar 8000 -ac 1 -c:a pcm_alaw audio/processed/alaw/welcome.wav
```

---

### 📝 Açıklamalar:

* `-i welcome.wav`: Giriş dosyası
* `-ar 8000`: 8000 Hz örnekleme hızı
* `-ac 1`: Mono kanal
* `-c:a pcm_mulaw`: Ses codec'i olarak **u-law** (alternatif olarak `pcm_alaw`)
* `.wav`: Çıkış formatı zaten WAV olacak şekilde ayarlandı

---

