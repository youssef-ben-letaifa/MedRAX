package handlers

import (
	"context"
	"encoding/json"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/bowang-lab/medrax-server/pkg/models"
	"github.com/bowang-lab/medrax-server/pkg/queue"
)

// ReasonHandler accepts new clinical reasoning requests
func ReasonHandler(pool *queue.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")

		if r.Method != http.MethodPost {
			http.Error(w, `{"error":"method not allowed; use POST"}`, http.StatusMethodNotAllowed)
			return
		}

		var req models.ReasonRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			w.WriteHeader(http.StatusBadRequest)
			_ = json.NewEncoder(w).Encode(map[string]string{"error": "invalid JSON payload"})
			return
		}

		if strings.TrimSpace(req.Query) == "" {
			w.WriteHeader(http.StatusBadRequest)
			_ = json.NewEncoder(w).Encode(map[string]string{"error": "query cannot be empty"})
			return
		}

		job, err := pool.Enqueue(req)
		if err != nil {
			if err == queue.ErrQueueFull {
				w.WriteHeader(http.StatusTooManyRequests)
				_ = json.NewEncoder(w).Encode(map[string]string{
					"error": "server is busy; queue capacity reached",
				})
				return
			}
			w.WriteHeader(http.StatusInternalServerError)
			_ = json.NewEncoder(w).Encode(map[string]string{"error": err.Error()})
			return
		}

		// If synchronous processing requested, wait with timeout
		if req.Sync {
			ctx, cancel := context.WithTimeout(r.Context(), 60*time.Second)
			defer cancel()

			completedJob, err := pool.WaitForJob(ctx, job.ID)
			if err != nil {
				w.WriteHeader(http.StatusGatewayTimeout)
				_ = json.NewEncoder(w).Encode(models.ReasonResponse{
					JobID:  job.ID,
					Status: models.StatusFailed,
					Error:  err.Error(),
				})
				return
			}

			w.WriteHeader(http.StatusOK)
			_ = json.NewEncoder(w).Encode(models.ReasonResponse{
				JobID:       completedJob.ID,
				Status:      completedJob.Status,
				Answer:      completedJob.Answer,
				ToolTraces:  completedJob.ToolTraces,
				DurationSec: completedJob.DurationSec,
				CreatedAt:   completedJob.CreatedAt,
				FinishedAt:  completedJob.FinishedAt,
			})
			return
		}

		// Asynchronous: return 202 Accepted
		w.WriteHeader(http.StatusAccepted)
		_ = json.NewEncoder(w).Encode(models.ReasonResponse{
			JobID:     job.ID,
			Status:    models.StatusQueued,
			CreatedAt: job.CreatedAt,
		})
	}
}

// JobStatusHandler retrieves the status of a specific job
func JobStatusHandler(pool *queue.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")

		if r.Method != http.MethodGet {
			http.Error(w, `{"error":"method not allowed; use GET"}`, http.StatusMethodNotAllowed)
			return
		}

		parts := strings.Split(strings.Trim(r.URL.Path, "/"), "/")
		if len(parts) < 4 {
			w.WriteHeader(http.StatusBadRequest)
			_ = json.NewEncoder(w).Encode(map[string]string{"error": "job ID required in path /api/v1/jobs/{id}"})
			return
		}
		jobID := parts[3]

		job, found := pool.GetJob(jobID)
		if !found {
			w.WriteHeader(http.StatusNotFound)
			_ = json.NewEncoder(w).Encode(map[string]string{"error": "job not found"})
			return
		}

		w.WriteHeader(http.StatusOK)
		_ = json.NewEncoder(w).Encode(models.ReasonResponse{
			JobID:       job.ID,
			Status:      job.Status,
			Answer:      job.Answer,
			ToolTraces:  job.ToolTraces,
			DurationSec: job.DurationSec,
			Error:       job.Error,
			CreatedAt:   job.CreatedAt,
			FinishedAt:  job.FinishedAt,
		})
	}
}

// JobsListHandler retrieves a list of recent jobs
func JobsListHandler(pool *queue.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")

		if r.Method != http.MethodGet {
			http.Error(w, `{"error":"method not allowed; use GET"}`, http.StatusMethodNotAllowed)
			return
		}

		limit := 50
		if qLimit := r.URL.Query().Get("limit"); qLimit != "" {
			if parsed, err := strconv.Atoi(qLimit); err == nil && parsed > 0 {
				limit = parsed
			}
		}

		jobs := pool.ListJobs(limit)
		w.WriteHeader(http.StatusOK)
		_ = json.NewEncoder(w).Encode(map[string]interface{}{
			"count": len(jobs),
			"jobs":  jobs,
		})
	}
}

