package openfiles

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
)

type Client struct { BaseURL string }

type DirEntry struct {
	Name string `json:"name"`
	Path string `json:"path"`
	Kind string `json:"kind"`
	Size uint64 `json:"size"`
}

func New(baseURL string) *Client {
	if baseURL == "" { baseURL = "http://127.0.0.1:8787" }
	return &Client{BaseURL: strings.TrimRight(baseURL, "/")}
}

func (c *Client) endpoint(prefix, path string) string {
	clean := strings.TrimLeft(path, "/")
	if clean == "" { return c.BaseURL + prefix }
	return c.BaseURL + prefix + "/" + url.PathEscape(clean)
}

func (c *Client) List(path string) ([]DirEntry, error) {
	resp, err := http.Get(c.endpoint("/v1/list", path))
	if err != nil { return nil, err }
	defer resp.Body.Close()
	if resp.StatusCode >= 300 { b,_ := io.ReadAll(resp.Body); return nil, fmt.Errorf("openfiles: %s", b) }
	var out []DirEntry
	return out, json.NewDecoder(resp.Body).Decode(&out)
}

func (c *Client) Read(path string) ([]byte, error) {
	resp, err := http.Get(c.endpoint("/v1/read", path))
	if err != nil { return nil, err }
	defer resp.Body.Close()
	if resp.StatusCode >= 300 { b,_ := io.ReadAll(resp.Body); return nil, fmt.Errorf("openfiles: %s", b) }
	return io.ReadAll(resp.Body)
}

func (c *Client) Write(path string, data []byte) error {
	req, err := http.NewRequest(http.MethodPut, c.endpoint("/v1/write", path), bytes.NewReader(data))
	if err != nil { return err }
	resp, err := http.DefaultClient.Do(req)
	if err != nil { return err }
	defer resp.Body.Close()
	if resp.StatusCode >= 300 { b,_ := io.ReadAll(resp.Body); return fmt.Errorf("openfiles: %s", b) }
	return nil
}

func (c *Client) Flush() error {
	resp, err := http.Post(c.BaseURL+"/v1/flush", "application/json", nil)
	if err != nil { return err }
	defer resp.Body.Close()
	if resp.StatusCode >= 300 { b,_ := io.ReadAll(resp.Body); return fmt.Errorf("openfiles: %s", b) }
	return nil
}
