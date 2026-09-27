// SPDX-License-Identifier: GPL-3.0-or-later
// The C++ the interface needs (D-094, spike 5): an asynchronous image provider, because a QML Image
// cannot take bytes from Rust directly, and a translation loader, because cxx-qt-lib has no QTranslator.
// Everything else is Rust and QML.
#include <QCoreApplication>
#include <QFontDatabase>
#include <QImage>
#include <QKeySequence>
#include <QQmlApplicationEngine>
#include <QQmlEngine>
#include <QQuickImageProvider>
#include <QQuickTextureFactory>
#include <QTimer>
#include <QTranslator>
#include <algorithm>
#include <atomic>
#include <cstring>
#include <mutex>
#include <unordered_map>

extern "C" {
void auroraw_thumbnail_request(int kind, const unsigned char *id, size_t len,
                               unsigned long long token);
}

namespace {
QTranslator *g_translator = nullptr;

class ThumbResponse;

// The responses waiting for their thumbnail, by the token the Rust side answers to. The collector's
// thread looks one up and posts to it under the lock, and a response leaves the table (under the
// lock) before it is destroyed, so an answer never reaches a response that is gone.
std::mutex g_mutex;
std::unordered_map<unsigned long long, ThumbResponse *> g_responses;
std::atomic<unsigned long long> g_next{1};

class ThumbResponse : public QQuickImageResponse {
public:
    ThumbResponse(int kind, const QString &id) : m_token(g_next++) {
        {
            std::lock_guard<std::mutex> lock(g_mutex);
            g_responses[m_token] = this;
        }
        const QByteArray name = id.toUtf8();
        auroraw_thumbnail_request(kind, reinterpret_cast<const unsigned char *>(name.constData()),
                                  static_cast<size_t>(name.size()), m_token);
    }

    ~ThumbResponse() override { forget(); }

    QQuickTextureFactory *textureFactory() const override {
        return m_image.isNull() ? nullptr : QQuickTextureFactory::textureFactoryForImage(m_image);
    }

    QString errorString() const override {
        return m_image.isNull() ? QStringLiteral("no thumbnail") : QString();
    }

    // The cell went away or scrolled off: nobody needs the answer any more.
    void cancel() override { forget(); }

    void done(const QImage &image) {
        m_image = image;
        emit finished();
    }

private:
    void forget() {
        std::lock_guard<std::mutex> lock(g_mutex);
        g_responses.erase(m_token);
    }

    unsigned long long m_token;
    QImage m_image;
};

// A request for a thumbnail does not wait: the answer arrives from the collector's thread, so the
// thumbnails of a screenful are made side by side and none of Qt's image threads is held up.
// The same provider serves the thumbnails (`image://thumbs`, kind 0) and the pictures of the image view
// (`image://preview`, kind 1): only the Rust side that answers differs.
class ThumbProvider : public QQuickAsyncImageProvider {
public:
    explicit ThumbProvider(int kind) : m_kind(kind) {}
    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &) override {
        return new ThumbResponse(m_kind, id);
    }

private:
    int m_kind;
};
} // namespace

// Called by the Rust collector, on its own thread: `bytes` is the JPEG (null when there is no
// thumbnail). Decoding happens here, off the GUI thread, and the finished image is handed over.
extern "C" void auroraw_thumbnail_ready(unsigned long long token, const unsigned char *bytes,
                                        size_t len) {
    QImage image;
    if (bytes) {
        image = QImage::fromData(bytes, static_cast<int>(len)); // JPEG, or PNG for an overlay
    }
    std::lock_guard<std::mutex> lock(g_mutex);
    const auto found = g_responses.find(token);
    if (found == g_responses.end()) {
        return;
    }
    ThumbResponse *response = found->second;
    QMetaObject::invokeMethod(
        response, [response, image]() { response->done(image); }, Qt::QueuedConnection);
}

// Lets the engine find the application's QML module (`import org.auroraw.ui`, with its singleton `Theme`)
// where the build put it, in the executable's resources: Qt 6.5 looks there by itself, 6.4 does not.
extern "C" void auroraw_add_import_path(void *engine, const unsigned char *path, size_t len) {
    static_cast<QQmlApplicationEngine *>(engine)->addImportPath(
        QString::fromUtf8(reinterpret_cast<const char *>(path), static_cast<qsizetype>(len)));
}

// Makes a font the application carries (TrueType bytes that stay valid for the life of the program)
// available by its family name.
extern "C" void auroraw_add_font(const unsigned char *data, size_t len) {
    QFontDatabase::addApplicationFontFromData(
        QByteArray(reinterpret_cast<const char *>(data), static_cast<qsizetype>(len)));
}

// Tests: quits the application after `ms` milliseconds of its event loop.
extern "C" void auroraw_quit_after(int ms) {
    QTimer::singleShot(ms, QCoreApplication::instance(), &QCoreApplication::quit);
}

// Registers `image://thumbs` on the engine of `object` (an object made by QML: the launcher, at the
// start of the window), unless it is there already. The application and QtQuickTest's runner make
// their engines differently, so the provider is installed from inside the engine.
extern "C" void auroraw_install_thumbnails(QObject *object) {
    if (QQmlEngine *engine = qmlEngine(object)) {
        if (!engine->imageProvider(QStringLiteral("thumbs"))) {
            engine->addImageProvider(QStringLiteral("thumbs"), new ThumbProvider(0));
        }
        if (!engine->imageProvider(QStringLiteral("preview"))) {
            engine->addImageProvider(QStringLiteral("preview"), new ThumbProvider(1));
        }
        // The overlays of the image view's picture (quality aids): the focus peaking and the clipping.
        if (!engine->imageProvider(QStringLiteral("peaking"))) {
            engine->addImageProvider(QStringLiteral("peaking"), new ThumbProvider(2));
        }
        if (!engine->imageProvider(QStringLiteral("clipping"))) {
            engine->addImageProvider(QStringLiteral("clipping"), new ThumbProvider(3));
        }
    }
}

// `data` stays valid for the life of the program (it is static in the Rust binary); null removes the
// current translation. `object`, when given, is an object made by QML: its engine retranslates what is
// on screen (QQmlEngine::retranslate must be called after a translator is installed).
extern "C" void auroraw_set_translation(const unsigned char *data, size_t len, QObject *object) {
    if (g_translator) {
        QCoreApplication::removeTranslator(g_translator);
        delete g_translator;
        g_translator = nullptr;
    }
    if (data && len > 0) {
        g_translator = new QTranslator(QCoreApplication::instance());
        if (g_translator->load(data, static_cast<int>(len))) {
            QCoreApplication::installTranslator(g_translator);
        } else {
            delete g_translator;
            g_translator = nullptr;
        }
    }
    if (object) {
        if (QQmlEngine *engine = qmlEngine(object)) {
            engine->retranslate();
        }
    }
}


// A shortcut written the way this platform writes it (Ctrl+N, ⌘N). `standard` is a
// QKeySequence::StandardKey, or negative to read `text` (UTF-8, "Ctrl+I") as a sequence. Qt Quick's
// own Shortcut item can say it too, but warns for every key with several bindings (Undo, Cut...).
// Returns the length of the UTF-8 answer, cut to `cap` bytes.
extern "C" size_t auroraw_shortcut_text(int standard, const unsigned char *text, size_t len,
                                        unsigned char *out, size_t cap) {
    const QKeySequence sequence =
        standard >= 0 ? QKeySequence(static_cast<QKeySequence::StandardKey>(standard))
                      : QKeySequence(QString::fromUtf8(reinterpret_cast<const char *>(text),
                                                       static_cast<qsizetype>(len)));
    const QByteArray answer = sequence.toString(QKeySequence::NativeText).toUtf8();
    const size_t n = std::min(static_cast<size_t>(answer.size()), cap);
    memcpy(out, answer.constData(), n);
    return n;
}
