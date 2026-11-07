// TorrentSharp.cpp : the tsdl C API over libtorrent.
// Created by Sakib. 25thDec, 2025
//
// No C++ exception may cross the extern "C" boundary: every export runs inside guard(),
// which turns an exception into an error code plus tsdl_last_error().

#include "TorrentSharp.h"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstring>
#include <deque>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include <libtorrent/alert_types.hpp>
#include <libtorrent/fingerprint.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/read_resume_data.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/torrent_handle.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>
#include <libtorrent/write_resume_data.hpp>

#ifndef TSDL_VERSION_STRING
#define TSDL_VERSION_STRING "0.0.0"
#endif

namespace
{
	constexpr auto resume_wait = std::chrono::seconds(5);
	constexpr size_t max_queued_events = 8192;

	thread_local std::string last_error;

	template <class R, class F>
	R guard(R failure, F&& body) noexcept
	{
		try
		{
			last_error.clear();
			return body();
		}
		catch (const std::exception& e)
		{
			last_error = e.what();
		}
		catch (...)
		{
			last_error = "unknown native exception";
		}
		return failure;
	}

	int32_t fail(int32_t code, const char* message)
	{
		last_error = message;
		return code;
	}

	template <class T>
	T* fail_null(const char* message)
	{
		last_error = message;
		return nullptr;
	}

	std::string to_string(const char* s)
	{
		return s == nullptr ? std::string() : std::string(s);
	}

	std::filesystem::path to_path(const std::string& utf8)
	{
		return std::filesystem::path(std::u8string(utf8.begin(), utf8.end()));
	}

	struct torrent_state
	{
		int64_t id = 0;
		std::string resume_path;
		int pending_resume = 0;
	};
}

struct tsdl_torrent_info
{
	std::shared_ptr<const lt::torrent_info> info;
};

struct tsdl_torrent
{
	tsdl_session* session = nullptr;
	lt::torrent_handle handle;
	std::shared_ptr<torrent_state> state;
};

struct tsdl_session
{
	std::unique_ptr<lt::session> ses;
	std::thread alert_thread;
	std::atomic<bool> stopping{ false };
	std::atomic<int64_t> next_id{ 1 };

	std::mutex mutex;
	std::condition_variable events_changed;
	std::condition_variable resume_changed;
	std::deque<tsdl_event> events;
	std::unordered_map<lt::torrent_handle, std::shared_ptr<torrent_state>> states;
	std::unordered_set<tsdl_torrent*> torrents;

	void push_event(int32_t type, int64_t torrent_id, int32_t piece, const std::string& message)
	{
		tsdl_event e{};
		e.type = type;
		e.piece = piece;
		e.torrent_id = torrent_id;
		const auto n = std::min(message.size(), static_cast<size_t>(TSDL_EVENT_MESSAGE_SIZE - 1));
		std::memcpy(e.message, message.data(), n);

		if (events.size() >= max_queued_events) events.pop_front();
		events.push_back(e);
	}

	std::shared_ptr<torrent_state> state_of(const lt::torrent_handle& h)
	{
		const auto it = states.find(h);
		return it == states.end() ? nullptr : it->second;
	}

	void request_resume(const lt::torrent_handle& h, torrent_state& state)
	{
		if (state.resume_path.empty() || !h.is_valid()) return;
		h.save_resume_data(lt::torrent_handle::save_info_dict | lt::torrent_handle::only_if_modified);
		state.pending_resume++;
	}

	static void write_resume_file(const std::string& path, const lt::add_torrent_params& params)
	{
		const auto buffer = lt::write_resume_data_buf(params);
		const auto target = to_path(path);
		auto temp = target;
		temp += ".tmp";
		{
			std::ofstream of(temp, std::ios_base::binary | std::ios_base::trunc);
			if (!of) return;
			of.write(buffer.data(), static_cast<std::streamsize>(buffer.size()));
			if (!of) return;
		}
		std::error_code ec;
		std::filesystem::rename(temp, target, ec);
	}

	void handle_alert(lt::alert* a)
	{
		if (const auto rd = lt::alert_cast<lt::save_resume_data_alert>(a))
		{
			std::string path;
			int64_t id = 0;
			{
				std::lock_guard lock(mutex);
				if (const auto state = state_of(rd->handle))
				{
					path = state->resume_path;
					id = state->id;
				}
			}
			if (!path.empty())
			{
				try { write_resume_file(path, rd->params); }
				catch (...) {}
			}

			std::lock_guard lock(mutex);
			if (const auto state = state_of(rd->handle))
			{
				state->pending_resume = std::max(0, state->pending_resume - 1);
				push_event(TSDL_EVENT_RESUME_SAVED, id, -1, {});
			}
			resume_changed.notify_all();
			events_changed.notify_all();
			return;
		}

		if (const auto rf = lt::alert_cast<lt::save_resume_data_failed_alert>(a))
		{
			std::lock_guard lock(mutex);
			if (const auto state = state_of(rf->handle))
				state->pending_resume = std::max(0, state->pending_resume - 1);
			resume_changed.notify_all();
			return;
		}

		std::lock_guard lock(mutex);
		if (const auto pf = lt::alert_cast<lt::piece_finished_alert>(a))
		{
			if (const auto state = state_of(pf->handle))
				push_event(TSDL_EVENT_PIECE_FINISHED, state->id, static_cast<int32_t>(pf->piece_index), {});
		}
		else if (const auto md = lt::alert_cast<lt::metadata_received_alert>(a))
		{
			if (const auto state = state_of(md->handle))
				push_event(TSDL_EVENT_METADATA_RECEIVED, state->id, -1, {});
		}
		else if (const auto te = lt::alert_cast<lt::torrent_error_alert>(a))
		{
			if (const auto state = state_of(te->handle))
				push_event(TSDL_EVENT_TORRENT_ERROR, state->id, -1, te->message());
		}
		else if (const auto fe = lt::alert_cast<lt::file_error_alert>(a))
		{
			if (const auto state = state_of(fe->handle))
				push_event(TSDL_EVENT_FILE_ERROR, state->id, -1, fe->message());
		}
		else if (const auto tf = lt::alert_cast<lt::torrent_finished_alert>(a))
		{
			if (const auto state = state_of(tf->handle))
				push_event(TSDL_EVENT_TORRENT_FINISHED, state->id, -1, {});
		}
		else
		{
			return;
		}
		events_changed.notify_all();
	}

	void run_alerts()
	{
		std::vector<lt::alert*> alerts;
		while (!stopping.load())
		{
			ses->wait_for_alert(lt::milliseconds(250));
			ses->pop_alerts(&alerts);
			for (const auto a : alerts)
			{
				try { handle_alert(a); }
				catch (...) {}
			}
		}
	}

	// waits until the given states (or all, when null) have no resume request in flight
	void wait_resume(const torrent_state* only)
	{
		std::unique_lock lock(mutex);
		resume_changed.wait_for(lock, resume_wait, [&]
			{
				if (only != nullptr) return only->pending_resume == 0;
				return std::ranges::all_of(states, [](const auto& kv) { return kv.second->pending_resume == 0; });
			});
	}
};

namespace
{
	std::vector<char> read_file(const std::string& path)
	{
		if (path.empty()) return {};
		std::ifstream ifs(to_path(path), std::ios_base::binary);
		if (!ifs) return {};
		return { std::istreambuf_iterator<char>(ifs), std::istreambuf_iterator<char>() };
	}

	// replaces atp with the saved resume data when it belongs to the same torrent
	void apply_resume(lt::add_torrent_params& atp, const std::string& resume_path)
	{
		const auto buffer = read_file(resume_path);
		if (buffer.empty()) return;

		lt::error_code ec;
		auto resume = lt::read_resume_data(buffer, ec);
		if (ec) return;

		const bool same = atp.ti ? resume.info_hashes == atp.ti->info_hashes() : resume.info_hashes == atp.info_hashes;
		if (!same) return;

		if (atp.ti && !resume.ti) resume.ti = atp.ti;
		atp = std::move(resume);
	}

	tsdl_torrent* attach(tsdl_session* session, lt::add_torrent_params atp, const std::string& save_path, const std::string& resume_path)
	{
		apply_resume(atp, resume_path);
		if (!save_path.empty()) atp.save_path = save_path;

		auto torrent = std::make_unique<tsdl_torrent>();
		torrent->session = session;
		torrent->state = std::make_shared<torrent_state>();
		torrent->state->id = session->next_id++;
		torrent->state->resume_path = resume_path;

		// register before add_torrent so no early alert is missed
		std::lock_guard lock(session->mutex);
		torrent->handle = session->ses->add_torrent(std::move(atp));
		if (!torrent->handle.is_valid()) return fail_null<tsdl_torrent>("add_torrent returned an invalid handle");

		session->states[torrent->handle] = torrent->state;
		session->torrents.insert(torrent.get());
		return torrent.release();
	}

	bool valid(const tsdl_torrent* torrent)
	{
		return torrent != nullptr && torrent->handle.is_valid();
	}

	std::shared_ptr<const lt::torrent_info> metadata(const tsdl_torrent* torrent)
	{
		auto ti = torrent->handle.torrent_file();
		return ti && ti->is_valid() ? ti : nullptr;
	}

	void remove(tsdl_session* session, tsdl_torrent* torrent)
	{
		{
			std::lock_guard lock(session->mutex);
			session->states.erase(torrent->handle);
			session->torrents.erase(torrent);
		}
		if (torrent->handle.is_valid())
		{
			torrent->handle.pause();
			session->ses->remove_torrent(torrent->handle);
		}
		delete torrent;
	}
}

extern "C" {
	const char* tsdl_version(void)
	{
		return TSDL_VERSION_STRING;
	}

	const char* tsdl_last_error(void)
	{
		return last_error.c_str();
	}

	tsdl_session* tsdl_session_create(void)
	{
		return guard<tsdl_session*>(nullptr, []() -> tsdl_session*
			{
				lt::settings_pack settings;
				settings.set_str(lt::settings_pack::user_agent, std::string("UFlix/" TSDL_VERSION_STRING " libtorrent/") + lt::version());
				settings.set_str(lt::settings_pack::peer_fingerprint, lt::generate_fingerprint("UF", 1, 1, 0, 0));
				settings.set_int(lt::settings_pack::alert_mask,
					lt::alert_category::status | lt::alert_category::error | lt::alert_category::storage | lt::alert_category::piece_progress);
				settings.set_str(lt::settings_pack::dht_bootstrap_nodes,
					"dht.libtorrent.org:25401,router.bittorrent.com:6881,router.utorrent.com:6881,dht.transmissionbt.com:6881");
				settings.set_bool(lt::settings_pack::enable_dht, true);
				settings.set_int(lt::settings_pack::close_file_interval, 30);
				settings.set_bool(lt::settings_pack::close_redundant_connections, true);

				auto session = std::make_unique<tsdl_session>();
				session->ses = std::make_unique<lt::session>(std::move(settings));
				auto raw = session.get();
				session->alert_thread = std::thread([raw] { raw->run_alerts(); });
				return session.release();
			});
	}

	void tsdl_session_destroy(tsdl_session* session)
	{
		if (session == nullptr) return;
		guard(0, [&]() -> int
			{
				{
					std::lock_guard lock(session->mutex);
					for (const auto torrent : session->torrents)
					{
						if (torrent->handle.is_valid() && torrent->handle.need_save_resume_data())
							session->request_resume(torrent->handle, *torrent->state);
					}
				}
				session->wait_resume(nullptr);
				return 0;
			});

		session->stopping = true;
		if (session->alert_thread.joinable()) session->alert_thread.join();
		{
			std::lock_guard lock(session->mutex);
			session->events_changed.notify_all();
		}

		for (const auto torrent : session->torrents) delete torrent;
		session->torrents.clear();
		guard(0, [&]() -> int { session->ses.reset(); return 0; });
		delete session;
	}

	int32_t tsdl_poll_events(tsdl_session* session, tsdl_event* events, int32_t capacity, int32_t timeout_ms)
	{
		if (session == nullptr || events == nullptr || capacity <= 0) return fail(TSDL_E_ARGUMENT, "invalid argument");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				std::unique_lock lock(session->mutex);
				session->events_changed.wait_for(lock, std::chrono::milliseconds(std::max(0, timeout_ms)),
					[&] { return !session->events.empty() || session->stopping.load(); });

				int32_t count = 0;
				while (count < capacity && !session->events.empty())
				{
					events[count++] = session->events.front();
					session->events.pop_front();
				}
				return count;
			});
	}

	tsdl_torrent_info* tsdl_torrent_info_from_bytes(const char* data, int64_t length)
	{
		if (data == nullptr || length <= 0) return fail_null<tsdl_torrent_info>("invalid argument");
		return guard<tsdl_torrent_info*>(nullptr, [&]() -> tsdl_torrent_info*
			{
				const lt::span<const char> buffer(data, static_cast<std::ptrdiff_t>(length));
				auto info = std::make_shared<const lt::torrent_info>(buffer, lt::load_torrent_limits{}, lt::from_span);
				return new tsdl_torrent_info{ std::move(info) };
			});
	}

	tsdl_torrent_info* tsdl_torrent_info_from_file(const char* file_path)
	{
		if (file_path == nullptr) return fail_null<tsdl_torrent_info>("invalid argument");
		return guard<tsdl_torrent_info*>(nullptr, [&]() -> tsdl_torrent_info*
			{
				const auto buffer = read_file(file_path);
				if (buffer.empty()) return fail_null<tsdl_torrent_info>("cannot read torrent file");
				auto info = std::make_shared<const lt::torrent_info>(lt::span<const char>(buffer), lt::load_torrent_limits{}, lt::from_span);
				return new tsdl_torrent_info{ std::move(info) };
			});
	}

	void tsdl_torrent_info_destroy(tsdl_torrent_info* info)
	{
		delete info;
	}

	tsdl_torrent* tsdl_attach_magnet(tsdl_session* session, const char* magnet, const char* save_path, const char* resume_path)
	{
		if (session == nullptr || magnet == nullptr) return fail_null<tsdl_torrent>("invalid argument");
		return guard<tsdl_torrent*>(nullptr, [&]() -> tsdl_torrent*
			{
				lt::error_code ec;
				auto atp = lt::parse_magnet_uri(magnet, ec);
				if (ec) return fail_null<tsdl_torrent>(("invalid magnet: " + ec.message()).c_str());
				return attach(session, std::move(atp), to_string(save_path), to_string(resume_path));
			});
	}

	tsdl_torrent* tsdl_attach_torrent(tsdl_session* session, const tsdl_torrent_info* info, const char* save_path, const char* resume_path)
	{
		if (session == nullptr || info == nullptr || !info->info) return fail_null<tsdl_torrent>("invalid argument");
		return guard<tsdl_torrent*>(nullptr, [&]() -> tsdl_torrent*
			{
				lt::add_torrent_params atp;
				atp.ti = std::make_shared<lt::torrent_info>(*info->info);
				return attach(session, std::move(atp), to_string(save_path), to_string(resume_path));
			});
	}

	int32_t tsdl_detach_torrent(tsdl_session* session, tsdl_torrent* torrent)
	{
		if (session == nullptr || torrent == nullptr || torrent->session != session) return fail(TSDL_E_ARGUMENT, "invalid argument");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				try
				{
					bool wait = false;
					{
						std::lock_guard lock(session->mutex);
						if (torrent->handle.is_valid() && torrent->handle.need_save_resume_data())
						{
							session->request_resume(torrent->handle, *torrent->state);
							wait = true;
						}
					}
					if (wait) session->wait_resume(torrent->state.get());
				}
				catch (...) {}

				remove(session, torrent);
				return TSDL_OK;
			});
	}

	int64_t tsdl_torrent_id(const tsdl_torrent* torrent)
	{
		return torrent == nullptr ? 0 : torrent->state->id;
	}

	int32_t tsdl_get_file_list(tsdl_torrent* torrent, tsdl_file_list* file_list)
	{
		if (file_list == nullptr) return fail(TSDL_E_ARGUMENT, "invalid argument");
		*file_list = {};
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");

		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				const auto ti = metadata(torrent);
				if (!ti) return fail(TSDL_E_NO_METADATA, "metadata not received yet");

				const auto& files = ti->files();
				const auto num_files = files.num_files();
				auto list = std::make_unique<tsdl_file_info[]>(static_cast<size_t>(num_files));
				int32_t filled = 0;

				auto copy = [](const std::string& s)
					{
						auto out = new char[s.size() + 1]();
						std::ranges::copy(s, out);
						return out;
					};

				try
				{
					for (const auto i : files.file_range())
					{
						auto& item = list[static_cast<size_t>(static_cast<int32_t>(i))];
						item.index = static_cast<int32_t>(i);
						item.offset = files.file_offset(i);
						item.file_size = files.file_size(i);
						item.modified_time = static_cast<int64_t>(files.mtime(i));
						item.file_path_is_absolute = files.file_absolute_path(i) ? 1 : 0;
						item.pad_file = files.pad_file_at(i) ? 1 : 0;
						item.file_name = copy(std::string(files.file_name(i)));
						item.file_path = copy(files.file_path(i));
						filled++;
					}
				}
				catch (...)
				{
					for (int32_t k = 0; k < filled; k++)
					{
						delete[] list[k].file_name;
						delete[] list[k].file_path;
					}
					throw;
				}

				file_list->piece_length = ti->piece_length();
				file_list->piece_count = ti->num_pieces();
				file_list->length = num_files;
				file_list->files = list.release();
				return TSDL_OK;
			});
	}

	void tsdl_destroy_file_list(tsdl_file_list* file_list)
	{
		if (file_list == nullptr || file_list->files == nullptr) return;

		for (int32_t i = 0; i < file_list->length; i++)
		{
			delete[] file_list->files[i].file_name;
			delete[] file_list->files[i].file_path;
		}
		delete[] file_list->files;
		*file_list = {};
	}

	int32_t tsdl_get_file_priority(tsdl_torrent* torrent, int32_t file_index)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				const auto ti = metadata(torrent);
				if (!ti) return fail(TSDL_E_NO_METADATA, "metadata not received yet");
				if (file_index < 0 || file_index >= ti->num_files()) return fail(TSDL_E_RANGE, "file index out of range");
				return static_cast<int32_t>(static_cast<uint8_t>(torrent->handle.file_priority(lt::file_index_t(file_index))));
			});
	}

	int32_t tsdl_set_file_priority(tsdl_torrent* torrent, int32_t file_index, uint8_t priority)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				const auto ti = metadata(torrent);
				if (!ti) return fail(TSDL_E_NO_METADATA, "metadata not received yet");
				if (file_index < 0 || file_index >= ti->num_files()) return fail(TSDL_E_RANGE, "file index out of range");
				torrent->handle.file_priority(lt::file_index_t(file_index), lt::download_priority_t(std::min<uint8_t>(priority, 7)));
				return TSDL_OK;
			});
	}

	int32_t tsdl_has_piece(tsdl_torrent* torrent, int32_t piece)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				const auto ti = metadata(torrent);
				if (!ti) return fail(TSDL_E_NO_METADATA, "metadata not received yet");
				if (piece < 0 || piece >= ti->num_pieces()) return fail(TSDL_E_RANGE, "piece index out of range");
				return torrent->handle.have_piece(lt::piece_index_t(piece)) ? 1 : 0;
			});
	}

	int32_t tsdl_get_pieces(tsdl_torrent* torrent, uint8_t* have, int32_t capacity)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		if (have == nullptr && capacity > 0) return fail(TSDL_E_ARGUMENT, "invalid argument");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				const auto status = torrent->handle.status(lt::torrent_handle::query_pieces);
				if (!status.has_metadata) return fail(TSDL_E_NO_METADATA, "metadata not received yet");

				const auto count = status.pieces.size();
				const auto n = std::min(count, capacity);
				for (int32_t i = 0; i < n; i++)
					have[i] = status.pieces.get_bit(lt::piece_index_t(i)) ? 1 : 0;
				return static_cast<int32_t>(count);
			});
	}

	int32_t tsdl_focus_pieces(tsdl_torrent* torrent, int32_t start, int32_t end, int32_t last_piece)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				const auto ti = metadata(torrent);
				if (!ti) return fail(TSDL_E_NO_METADATA, "metadata not received yet");

				const int32_t count = ti->num_pieces();
				if (count == 0) return TSDL_OK;
				start = std::clamp(start, 0, count - 1);
				end = std::clamp(end, start, count - 1);
				last_piece = std::clamp(last_piece, end, count - 1);

				std::vector<lt::download_priority_t> priorities(static_cast<size_t>(count), lt::dont_download);
				for (int32_t i = start; i <= last_piece; i++)
					priorities[static_cast<size_t>(i)] = i <= end ? lt::top_priority : lt::low_priority;

				torrent->handle.clear_piece_deadlines();
				torrent->handle.prioritize_pieces(priorities);
				for (int32_t i = start; i <= end; i++)
					torrent->handle.set_piece_deadline(lt::piece_index_t(i), 2000 * (1 + i - start));
				return TSDL_OK;
			});
	}

	int32_t tsdl_reset_pieces(tsdl_torrent* torrent)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				const auto ti = metadata(torrent);
				if (!ti) return fail(TSDL_E_NO_METADATA, "metadata not received yet");

				torrent->handle.clear_piece_deadlines();
				torrent->handle.prioritize_pieces(std::vector<lt::download_priority_t>(static_cast<size_t>(ti->num_pieces()), lt::default_priority));
				return TSDL_OK;
			});
	}

	int32_t tsdl_save_resume_data(tsdl_torrent* torrent)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				std::lock_guard lock(torrent->session->mutex);
				torrent->session->request_resume(torrent->handle, *torrent->state);
				return TSDL_OK;
			});
	}

	int32_t tsdl_start_torrent(tsdl_torrent* torrent)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t { torrent->handle.resume(); return TSDL_OK; });
	}

	int32_t tsdl_stop_torrent(tsdl_torrent* torrent)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t { torrent->handle.pause(); return TSDL_OK; });
	}

	int32_t tsdl_get_status(tsdl_torrent* torrent, tsdl_status* status)
	{
		if (status == nullptr) return fail(TSDL_E_ARGUMENT, "invalid argument");
		*status = {};
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");

		return guard<int32_t>(TSDL_E_EXCEPTION, [&]() -> int32_t
			{
				const auto s = torrent->handle.status();

				if (s.errc)
				{
					status->state = TSDL_STATE_ERROR;
				}
				else
				{
					switch (s.state)
					{
					case lt::torrent_status::checking_files: status->state = TSDL_STATE_CHECKING; break;
					case lt::torrent_status::checking_resume_data: status->state = TSDL_STATE_CHECKING_RESUME; break;
					case lt::torrent_status::downloading_metadata: status->state = TSDL_STATE_METADATA; break;
					case lt::torrent_status::downloading: status->state = TSDL_STATE_DOWNLOADING; break;
					case lt::torrent_status::seeding: status->state = TSDL_STATE_SEEDING; break;
					case lt::torrent_status::finished: status->state = TSDL_STATE_FINISHED; break;
					default: status->state = TSDL_STATE_UNKNOWN; break;
					}
				}

				status->progress = s.progress;
				status->total_size = s.total;
				status->count_peers = s.num_peers;
				status->count_seeds = s.num_seeds;
				status->bytes_uploaded = s.all_time_upload;
				status->bytes_downloaded = s.all_time_download;
				status->upload_rate = s.upload_payload_rate;
				status->download_rate = s.download_payload_rate;
				return TSDL_OK;
			});
	}

	int64_t tsdl_file_progress(tsdl_torrent* torrent, int32_t file_index)
	{
		if (!valid(torrent)) return fail(TSDL_E_INVALID_HANDLE, "invalid torrent handle");
		return guard<int64_t>(TSDL_E_EXCEPTION, [&]() -> int64_t
			{
				const auto ti = metadata(torrent);
				if (!ti) return fail(TSDL_E_NO_METADATA, "metadata not received yet");
				if (file_index < 0 || file_index >= ti->num_files()) return fail(TSDL_E_RANGE, "file index out of range");

				const auto progress = torrent->handle.file_progress(lt::torrent_handle::piece_granularity);
				return progress[static_cast<size_t>(file_index)];
			});
	}
}
