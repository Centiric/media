# Media Servisi - Ses Dosyası Hazırlama

`media` servisinin anons çalabilmesi için, ses dosyalarının **standart 16-bit PCM WAV** formatında olması gerekmektedir.

---
### 🎯 Hedef Format Özellikleri:

*   **Codec:** Sıkıştırılmamış PCM (Signed 16-bit Little-Endian)
*   **Örnekleme hızı (Sample Rate):** 8000 Hz
*   **Kanal:** Mono
*   **Bit derinliği:** 16-bit

---
### 🛠️ `ffmpeg` ile Dönüştürme Komutu:

Aşağıdaki komut, herhangi bir ses dosyasını (`orjinal.wav`) bu standart formata çevirecektir.

```bash
ffmpeg -i audio/orjinal/welcome.wav -ar 8000 -ac 1 -acodec pcm_s16le audio/processed/standard/welcome.wav
```
*   `-acodec pcm_s16le`: Bu, "Signed 16-bit Little-Endian PCM" anlamına gelir ve en uyumlu formattır.

Oluşturduğunuz bu `standard/welcome.wav` dosyasını kullanacağız.
```

Şimdi bu komutu kullanarak `standard/welcome.wav` dosyasını oluşturun ve `media` projenize ekleyin.

#### Adım 2: `config/default.toml`'u Güncelleme

`media` projesindeki `config/default.toml` dosyasında, yeni ve doğru dosyayı işaret edelim:
```toml
[announcement]
welcome_file_path = "audio/processed/standard/welcome.wav"
```
