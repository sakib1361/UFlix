// TorrentSharp.h : the tsdl C API. Plain C types only, so the layout is identical on every OS
// and the header needs no libtorrent include. NativeRunner (C#) is the only consumer.

#pragma once
#include "export.h"
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

	typedef struct tsdl_session tsdl_session;
	typedef struct tsdl_torrent tsdl_torrent;
	typedef struct tsdl_torrent_info tsdl_torrent_info;

	// return codes: >= 0 success, < 0 failure (tsdl_last_error() has the message)
	enum tsdl_result {
		TSDL_OK = 0,
		TSDL_E_ARGUMENT = -1,
		TSDL_E_EXCEPTION = -2,
		TSDL_E_NO_METADATA = -3,
		TSDL_E_RANGE = -4,
		TSDL_E_INVALID_HANDLE = -5
	};

	typedef struct tsdl_file_info {
		int32_t index;
		int64_t offset;
		int64_t file_size;
		int64_t modified_time;
		char* file_name;
		char* file_path;
		uint8_t file_path_is_absolute;
		uint8_t pad_file;
	} tsdl_file_info;

	typedef struct tsdl_file_list {
		int32_t piece_length;
		int32_t piece_count;
		int32_t length;
		tsdl_file_info* files;
	} tsdl_file_list;

	enum tsdl_torrent_state {
		TSDL_STATE_UNKNOWN = 0,
		TSDL_STATE_CHECKING = 1,
		TSDL_STATE_CHECKING_RESUME = 2,
		TSDL_STATE_METADATA = 3,
		TSDL_STATE_DOWNLOADING = 4,
		TSDL_STATE_SEEDING = 5,
		TSDL_STATE_FINISHED = 6,
		TSDL_STATE_ERROR = 7
	};

	typedef struct tsdl_status {
		int32_t state;
		float progress;
		int64_t total_size;

		int32_t count_peers;
		int32_t count_seeds;

		int64_t bytes_uploaded;
		int64_t bytes_downloaded;

		int64_t upload_rate;
		int64_t download_rate;
	} tsdl_status;

	enum tsdl_event_type {
		TSDL_EVENT_PIECE_FINISHED = 1,
		TSDL_EVENT_METADATA_RECEIVED = 2,
		TSDL_EVENT_TORRENT_ERROR = 3,
		TSDL_EVENT_FILE_ERROR = 4,
		TSDL_EVENT_RESUME_SAVED = 5,
		TSDL_EVENT_TORRENT_FINISHED = 6
	};

	enum { TSDL_EVENT_MESSAGE_SIZE = 240 };

	typedef struct tsdl_event {
		int32_t type;
		int32_t piece;
		int64_t torrent_id;
		char message[TSDL_EVENT_MESSAGE_SIZE];
	} tsdl_event;

	TSDL_API const char* tsdl_version(void);
	// message of the last failed call on the calling thread; valid until the next call on that thread
	TSDL_API const char* tsdl_last_error(void);

	// session: owns a native alert thread that writes resume data and queues events
	TSDL_API tsdl_session* tsdl_session_create(void);
	// saves resume data of every attached torrent (bounded wait), then frees the session and its torrents
	TSDL_API void tsdl_session_destroy(tsdl_session* session);
	// waits up to timeout_ms for events; returns how many were copied into events
	TSDL_API int32_t tsdl_poll_events(tsdl_session* session, tsdl_event* events, int32_t capacity, int32_t timeout_ms);

	TSDL_API tsdl_torrent_info* tsdl_torrent_info_from_file(const char* file_path);
	TSDL_API tsdl_torrent_info* tsdl_torrent_info_from_bytes(const char* data, int64_t length);
	TSDL_API void tsdl_torrent_info_destroy(tsdl_torrent_info* info);

	// resume_path may be null or empty; when set, resume data is read from and saved to it
	TSDL_API tsdl_torrent* tsdl_attach_magnet(tsdl_session* session, const char* magnet, const char* save_path, const char* resume_path);
	TSDL_API tsdl_torrent* tsdl_attach_torrent(tsdl_session* session, const tsdl_torrent_info* info, const char* save_path, const char* resume_path);
	// saves resume data (bounded wait), removes the torrent and frees the handle
	TSDL_API int32_t tsdl_detach_torrent(tsdl_session* session, tsdl_torrent* torrent);
	TSDL_API int64_t tsdl_torrent_id(const tsdl_torrent* torrent);

	// TSDL_E_NO_METADATA until the metadata has arrived
	TSDL_API int32_t tsdl_get_file_list(tsdl_torrent* torrent, tsdl_file_list* file_list);
	TSDL_API void tsdl_destroy_file_list(tsdl_file_list* file_list);

	TSDL_API int32_t tsdl_get_file_priority(tsdl_torrent* torrent, int32_t file_index);
	TSDL_API int32_t tsdl_set_file_priority(tsdl_torrent* torrent, int32_t file_index, uint8_t priority);

	// 1 = have, 0 = missing, < 0 = error
	TSDL_API int32_t tsdl_has_piece(tsdl_torrent* torrent, int32_t piece);
	// fills have[i] with 1/0 for up to capacity pieces; returns the torrent's piece count
	TSDL_API int32_t tsdl_get_pieces(tsdl_torrent* torrent, uint8_t* have, int32_t capacity);
	TSDL_API int32_t tsdl_focus_pieces(tsdl_torrent* torrent, int32_t start, int32_t end, int32_t last_piece);
	TSDL_API int32_t tsdl_reset_pieces(tsdl_torrent* torrent);

	// asynchronous: the alert thread writes the file named at attach time
	TSDL_API int32_t tsdl_save_resume_data(tsdl_torrent* torrent);

	TSDL_API int32_t tsdl_get_status(tsdl_torrent* torrent, tsdl_status* status);

	TSDL_API int32_t tsdl_start_torrent(tsdl_torrent* torrent);
	TSDL_API int32_t tsdl_stop_torrent(tsdl_torrent* torrent);

	// downloaded bytes of the file (piece granularity), < 0 on error
	TSDL_API int64_t tsdl_file_progress(tsdl_torrent* torrent, int32_t file_index);

#ifdef __cplusplus
}
#endif
